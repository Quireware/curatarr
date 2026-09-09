use axum::Router;
use axum::body::Body;
use axum::http::{HeaderMap, Method, Request, StatusCode};
use curatarr_api::router::build_router;
use curatarr_api::state::AppState;
use curatarr_config::library::LibraryConfig;
use curatarr_core::traits::repository::Repository;
use curatarr_core::types::edition::NewEdition;
use curatarr_core::types::enums::{ContentType, FileFormat, ReadStatus};
use curatarr_core::types::file::NewLibraryFile;
use curatarr_core::types::work::NewWork;
use curatarr_db::create_repository;
use http_body_util::BodyExt;
use serde_json::{Value, json};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tower::ServiceExt;

struct TestApp {
    app: Router,
    db: Arc<dyn Repository>,
    _dir: tempfile::TempDir,
    root: std::path::PathBuf,
}

async fn test_app() -> TestApp {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("library");
    std::fs::create_dir_all(&root).unwrap();
    let db = create_repository("sqlite::memory:").await.unwrap();
    let library = LibraryConfig {
        data_dir: dir.path().join("data"),
        naming_template: "{Author}/{Title}.{Extension}".into(),
        ..Default::default()
    };
    TestApp {
        app: build_router(AppState::new(db.clone(), library)),
        db,
        _dir: dir,
        root,
    }
}

async fn call(app: &Router, method: Method, uri: &str, body: Option<Value>) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .header(
            "authorization",
            format!("Bearer {}", curatarr_api::state::TEST_API_TOKEN),
        )
        .body(match body {
            Some(v) => Body::from(v.to_string()),
            None => Body::empty(),
        })
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes)
            .unwrap_or(Value::String(String::from_utf8_lossy(&bytes).into_owned()))
    };
    (status, json)
}

fn work_body(title: &str, content_type: &str) -> Value {
    json!({
        "title": title,
        "sort_title": title,
        "original_language": null,
        "original_pub_date": null,
        "description": null,
        "description_html": null,
        "content_type": content_type,
        "age_rating": null,
        "content_warnings": [],
        "read_status": "unread"
    })
}

fn author_body(name: &str) -> Value {
    json!({
        "name": name,
        "sort_name": name,
        "birth_date": null,
        "death_date": null,
        "nationality": null,
        "biography": null,
        "biography_html": null,
        "external_ids": []
    })
}

#[tokio::test]
async fn work_crud_cycle() {
    let t = test_app().await;

    let (status, created) = call(
        &t.app,
        Method::POST,
        "/api/v1/works",
        Some(work_body("Dune", "book")),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(created["title"], "Dune");
    let id = created["id"].as_str().unwrap().to_string();

    let (status, fetched) = call(&t.app, Method::GET, &format!("/api/v1/works/{id}"), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(fetched["id"], id);

    let (status, updated) = call(
        &t.app,
        Method::PUT,
        &format!("/api/v1/works/{id}"),
        Some(json!({"title": "Dune Messiah", "description": "sequel", "read_status": "reading"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["title"], "Dune Messiah");
    assert_eq!(updated["description"], "sequel");
    assert_eq!(updated["read_status"], "reading");

    // JSON null clears a nullable field; absent keys are untouched.
    let (status, cleared) = call(
        &t.app,
        Method::PUT,
        &format!("/api/v1/works/{id}"),
        Some(json!({"description": null})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(cleared["description"], Value::Null);
    assert_eq!(cleared["title"], "Dune Messiah");

    let (status, _) = call(&t.app, Method::DELETE, &format!("/api/v1/works/{id}"), None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, err) = call(&t.app, Method::GET, &format!("/api/v1/works/{id}"), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(err["error"]["code"], "NOT_FOUND");
    assert!(err["error"]["message"].as_str().unwrap().contains(&id));
}

#[tokio::test]
async fn work_validation_and_pagination_and_filter() {
    let t = test_app().await;

    let (status, err) = call(
        &t.app,
        Method::POST,
        "/api/v1/works",
        Some(work_body("   ", "book")),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(err["error"]["code"], "BAD_REQUEST");

    for (title, ct) in [
        ("A", "book"),
        ("B", "manga"),
        ("C", "manga"),
        ("D", "comic"),
    ] {
        let (status, _) = call(
            &t.app,
            Method::POST,
            "/api/v1/works",
            Some(work_body(title, ct)),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);
    }

    let (status, page) = call(&t.app, Method::GET, "/api/v1/works?page=1&per_page=2", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(page["total"], 4);
    assert_eq!(page["items"].as_array().unwrap().len(), 2);
    assert_eq!(page["per_page"], 2);

    let (status, manga) = call(
        &t.app,
        Method::GET,
        "/api/v1/works?content_type=manga",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(manga["total"], 2);

    let (status, by_title) = call(&t.app, Method::GET, "/api/v1/works?title=D", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(by_title["total"], 1);

    // per_page is clamped rather than rejected
    let (status, clamped) = call(&t.app, Method::GET, "/api/v1/works?per_page=99999", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(clamped["per_page"], 500);
}

#[tokio::test]
async fn work_author_links_and_editions() {
    let t = test_app().await;
    let (_, work) = call(
        &t.app,
        Method::POST,
        "/api/v1/works",
        Some(work_body("Dune", "book")),
    )
    .await;
    let work_id = work["id"].as_str().unwrap().to_string();
    let (status, author) = call(
        &t.app,
        Method::POST,
        "/api/v1/authors",
        Some(author_body("Frank Herbert")),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let author_id = author["id"].as_str().unwrap().to_string();

    let (status, _) = call(
        &t.app,
        Method::POST,
        &format!("/api/v1/works/{work_id}/authors"),
        Some(json!({"author_id": author_id, "role": "author"})),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (_, authors) = call(
        &t.app,
        Method::GET,
        &format!("/api/v1/works/{work_id}/authors"),
        None,
    )
    .await;
    assert_eq!(authors.as_array().unwrap().len(), 1);
    assert_eq!(authors[0]["name"], "Frank Herbert");

    let (status, _) = call(
        &t.app,
        Method::POST,
        &format!("/api/v1/works/{work_id}/authors"),
        Some(json!({"author_id": uuid::Uuid::now_v7()})),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, edition) = call(
        &t.app,
        Method::POST,
        "/api/v1/editions",
        Some(json!({
            "work_id": work_id, "isbn13": "9780306406157", "isbn10": null, "asin": null,
            "publisher_id": null, "imprint": null, "publication_date": null, "edition_number": 1,
            "format": "epub", "page_count": 412, "word_count": null, "language": "en", "translator": null
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(edition["isbn13"], "9780306406157");

    let (_, editions) = call(
        &t.app,
        Method::GET,
        &format!("/api/v1/works/{work_id}/editions"),
        None,
    )
    .await;
    assert_eq!(editions["total"], 1);

    let (status, _) = call(
        &t.app,
        Method::DELETE,
        &format!("/api/v1/works/{work_id}/authors/{author_id}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, authors) = call(
        &t.app,
        Method::GET,
        &format!("/api/v1/works/{work_id}/authors"),
        None,
    )
    .await;
    assert!(authors.as_array().unwrap().is_empty());
}

#[tokio::test]
async fn series_entries_cycle() {
    let t = test_app().await;
    let (_, work) = call(
        &t.app,
        Method::POST,
        "/api/v1/works",
        Some(work_body("Dune", "book")),
    )
    .await;
    let work_id = work["id"].as_str().unwrap().to_string();
    let (status, series) = call(
        &t.app,
        Method::POST,
        "/api/v1/series",
        Some(json!({
            "title": "Dune Chronicles", "sort_title": "Dune Chronicles", "description": null,
            "series_type": "completed", "reading_order": "publication",
            "volume_count": 6, "expected_volume_count": 6, "external_ids": []
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let series_id = series["id"].as_str().unwrap().to_string();

    let (status, entry) = call(
        &t.app,
        Method::POST,
        &format!("/api/v1/series/{series_id}/entries"),
        Some(json!({"work_id": work_id, "position": 1.0})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let entry_id = entry["id"].as_str().unwrap().to_string();

    let (_, entries) = call(
        &t.app,
        Method::GET,
        &format!("/api/v1/series/{series_id}/entries"),
        None,
    )
    .await;
    assert_eq!(entries.as_array().unwrap().len(), 1);
    let (_, from_work) = call(
        &t.app,
        Method::GET,
        &format!("/api/v1/works/{work_id}/series"),
        None,
    )
    .await;
    assert_eq!(from_work[0]["series_id"], series_id);

    let (status, _) = call(
        &t.app,
        Method::DELETE,
        &format!("/api/v1/series/{series_id}/entries/{entry_id}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
}

async fn wait_for_scan(app: &Router, folder_id: &str) -> Value {
    for _ in 0..200 {
        let (status, body) = call(
            app,
            Method::GET,
            &format!("/api/v1/root-folders/{folder_id}/scan/status"),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        if body["state"] != "running" {
            return body;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("scan did not finish");
}

fn write_fake_pdf(path: &Path, salt: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, format!("%PDF-1.4\n% {salt}\n")).unwrap();
}

#[tokio::test]
async fn root_folder_create_scan_and_status() {
    let t = test_app().await;

    let (status, err) = call(
        &t.app,
        Method::POST,
        "/api/v1/root-folders",
        Some(json!({"path": "relative/books"})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(err["error"]["code"], "BAD_REQUEST");

    let (status, folder) = call(
        &t.app,
        Method::POST,
        "/api/v1/root-folders",
        Some(json!({"path": t.root, "name": "Books", "content_types": ["book"]})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(folder["accessible"], true);
    assert!(folder["free_space_bytes"].as_u64().is_some());
    let folder_id = folder["id"].as_str().unwrap().to_string();

    let (status, _) = call(
        &t.app,
        Method::POST,
        "/api/v1/root-folders",
        Some(json!({"path": t.root})),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);

    let (status, not_yet) = call(
        &t.app,
        Method::GET,
        &format!("/api/v1/root-folders/{folder_id}/scan/status"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(not_yet["error"]["code"], "NOT_FOUND");

    write_fake_pdf(&t.root.join("Some Book.pdf"), "one");
    write_fake_pdf(&t.root.join("sub/Another Book.pdf"), "two");
    std::fs::write(t.root.join("notes.txt"), "ignored").unwrap();

    let (status, accepted) = call(
        &t.app,
        Method::POST,
        &format!("/api/v1/root-folders/{folder_id}/scan"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(accepted["root_folder_id"], folder_id);

    let done = wait_for_scan(&t.app, &folder_id).await;
    assert_eq!(done["state"], "completed");
    assert_eq!(done["total"], 2);
    assert_eq!(done["imported"], 2);
    assert_eq!(done["failed"], 0);

    let (_, works) = call(&t.app, Method::GET, "/api/v1/works", None).await;
    assert_eq!(works["total"], 2);
    let titles: Vec<&str> = works["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w["title"].as_str().unwrap())
        .collect();
    assert!(titles.contains(&"Some Book"));
    assert!(titles.contains(&"Another Book"));

    // in-place scan: files stay put
    assert!(t.root.join("Some Book.pdf").exists());
    let (_, files) = call(&t.app, Method::GET, "/api/v1/files", None).await;
    assert_eq!(files["total"], 2);

    // rescanning finds only duplicates
    let (status, _) = call(
        &t.app,
        Method::POST,
        &format!("/api/v1/root-folders/{folder_id}/scan"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let again = wait_for_scan(&t.app, &folder_id).await;
    assert_eq!(again["imported"], 0);
    assert_eq!(again["duplicates"], 2);

    let (status, _) = call(
        &t.app,
        Method::DELETE,
        &format!("/api/v1/root-folders/{folder_id}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, list) = call(&t.app, Method::GET, "/api/v1/root-folders", None).await;
    assert!(list.as_array().unwrap().is_empty());
}

#[tokio::test]
async fn import_into_root_folder_organises_files() {
    let t = test_app().await;
    let (_, folder) = call(
        &t.app,
        Method::POST,
        "/api/v1/root-folders",
        Some(json!({"path": t.root})),
    )
    .await;
    let folder_id = folder["id"].as_str().unwrap().to_string();

    let inbox = t._dir.path().join("inbox");
    write_fake_pdf(&inbox.join("Fresh Book.pdf"), "fresh");

    let (status, summary) = call(
        &t.app,
        Method::POST,
        &format!("/api/v1/root-folders/{folder_id}/import"),
        Some(json!({"source": inbox})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{summary}");
    assert_eq!(summary["imported"], 1);
    assert!(t.root.join("Unknown Author/Fresh Book.pdf").exists());
    // default import mode is copy
    assert!(inbox.join("Fresh Book.pdf").exists());

    let (status, err) = call(
        &t.app,
        Method::POST,
        &format!("/api/v1/root-folders/{folder_id}/import"),
        Some(json!({"source": "not/absolute"})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(err["error"]["code"], "BAD_REQUEST");
}

async fn seed_file(t: &TestApp, title: &str, path: &Path, sha256: &str) -> String {
    let work =
        t.db.create_work(&NewWork {
            title: title.into(),
            sort_title: title.into(),
            original_language: None,
            original_pub_date: None,
            description: None,
            description_html: None,
            content_type: ContentType::Book,
            age_rating: None,
            content_warnings: vec![],
            read_status: ReadStatus::Unread,
            monitored: false,
        })
        .await
        .unwrap();
    let edition =
        t.db.create_edition(&NewEdition {
            work_id: work.id,
            isbn13: None,
            isbn10: None,
            asin: None,
            publisher_id: None,
            imprint: None,
            publication_date: None,
            edition_number: None,
            format: FileFormat::Pdf,
            page_count: None,
            word_count: None,
            language: None,
            translator: None,
        })
        .await
        .unwrap();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, b"bytes").unwrap();
    t.db.create_file(&NewLibraryFile {
        edition_id: edition.id,
        path: path.to_string_lossy().into_owned(),
        format: FileFormat::Pdf,
        size_bytes: 5,
        sha256: sha256.into(),
    })
    .await
    .unwrap()
    .id
    .to_string()
}

async fn call_raw(
    app: &Router,
    method: Method,
    uri: &str,
    headers: Vec<(&str, &str)>,
    body: Option<Value>,
) -> (StatusCode, HeaderMap, Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json");
    for (k, v) in headers {
        builder = builder.header(k, v);
    }
    let request = builder
        .body(match body {
            Some(v) => Body::from(v.to_string()),
            None => Body::empty(),
        })
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let header_map = response.headers().clone();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes)
            .unwrap_or(Value::String(String::from_utf8_lossy(&bytes).into_owned()))
    };
    (status, header_map, json)
}

#[tokio::test]
async fn api_without_bearer_is_unauthorized() {
    let t = test_app().await;
    let request = Request::builder()
        .method(Method::GET)
        .uri("/api/v1/works")
        .body(Body::empty())
        .unwrap();
    let response = t.app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn login_cookie_reaches_wanted_and_login_is_public() {
    let t = test_app().await;
    t.db.create_user("admin", &curatarr_auth::hash_password("secret").unwrap())
        .await
        .unwrap();

    let (status, _, _) = call_raw(&t.app, Method::GET, "/login", vec![], None).await;
    assert_eq!(status, StatusCode::OK);

    let (status, headers, body) = call_raw(
        &t.app,
        Method::POST,
        "/api/v1/login",
        vec![],
        Some(json!({"username":"admin","password":"secret"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let csrf = body["csrf_token"].as_str().unwrap().to_string();
    let cookie = headers
        .get("set-cookie")
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();

    let (status, _, wanted) = call_raw(
        &t.app,
        Method::GET,
        "/api/v1/wanted",
        vec![("cookie", cookie.as_str())],
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(wanted.as_array().unwrap().is_empty());

    let (status, created) = call(
        &t.app,
        Method::POST,
        "/api/v1/works",
        Some(work_body("Dune", "book")),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let id = created["id"].as_str().unwrap();

    let (status, _, _) = call_raw(
        &t.app,
        Method::POST,
        &format!("/api/v1/works/{id}/monitor"),
        vec![("cookie", cookie.as_str()), ("x-csrf-token", csrf.as_str())],
        Some(json!({"monitored": true})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn login_lockout_survives_in_database() {
    let t = test_app().await;
    t.db.create_user("admin", &curatarr_auth::hash_password("secret").unwrap())
        .await
        .unwrap();
    for _ in 0..10 {
        let (status, _, _) = call_raw(
            &t.app,
            Method::POST,
            "/api/v1/login",
            vec![],
            Some(json!({"username":"admin","password":"wrong"})),
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }
    let (status, _, err) = call_raw(
        &t.app,
        Method::POST,
        "/api/v1/login",
        vec![],
        Some(json!({"username":"admin","password":"secret"})),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(err["error"]["code"], "LOCKED");
}

#[tokio::test]
async fn metadata_refresh_and_locks_and_providers() {
    let t = test_app().await;
    let (status, created) = call(
        &t.app,
        Method::POST,
        "/api/v1/works",
        Some(work_body("Dune", "book")),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let id = created["id"].as_str().unwrap();

    let (status, providers) = call(&t.app, Method::GET, "/api/v1/system/providers", None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(providers.as_array().unwrap().is_empty());

    let (status, report) = call(
        &t.app,
        Method::POST,
        &format!("/api/v1/works/{id}/refresh-metadata"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(report["applied"].as_array().unwrap().len(), 0);
    assert_eq!(report["preview"], false);

    let (status, lock) = call(
        &t.app,
        Method::POST,
        &format!("/api/v1/works/{id}/field-locks"),
        Some(json!({"field": "title"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(lock["field"], "title");

    let (status, bad) = call(
        &t.app,
        Method::POST,
        &format!("/api/v1/works/{id}/field-locks"),
        Some(json!({"field": "user_notes"})),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(bad["error"]["code"], "BAD_REQUEST");

    let (status, _) = call(
        &t.app,
        Method::DELETE,
        &format!("/api/v1/works/{id}/field-locks/title"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn recycle_restore_and_permanent_delete() {
    let t = test_app().await;
    let path = t.root.join("Book.pdf");
    let file_id = seed_file(&t, "Book", &path, "h1").await;

    let (status, err) = call(
        &t.app,
        Method::DELETE,
        &format!("/api/v1/files/{file_id}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        err["error"]["message"]
            .as_str()
            .unwrap()
            .contains("confirm")
    );

    let (status, entry) = call(
        &t.app,
        Method::POST,
        &format!("/api/v1/files/{file_id}/recycle"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{entry}");
    assert!(!path.exists());
    assert!(Path::new(entry["recycle_path"].as_str().unwrap()).exists());

    let (status, _) = call(
        &t.app,
        Method::POST,
        &format!("/api/v1/files/{file_id}/recycle"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);

    let (_, bin) = call(&t.app, Method::GET, "/api/v1/recycle-bin", None).await;
    assert_eq!(bin.as_array().unwrap().len(), 1);

    let (_, active) = call(&t.app, Method::GET, "/api/v1/files", None).await;
    assert_eq!(active["total"], 0);
    let (_, all) = call(
        &t.app,
        Method::GET,
        "/api/v1/files?include_deleted=true",
        None,
    )
    .await;
    assert_eq!(all["total"], 1);

    let (status, restored) = call(
        &t.app,
        Method::POST,
        &format!("/api/v1/files/{file_id}/restore"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{restored}");
    assert_eq!(restored["deleted_at"], Value::Null);
    assert!(path.exists());

    let (status, _) = call(
        &t.app,
        Method::POST,
        &format!("/api/v1/files/{file_id}/recycle"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = call(
        &t.app,
        Method::DELETE,
        &format!("/api/v1/files/{file_id}?confirm=true"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = call(
        &t.app,
        Method::GET,
        &format!("/api/v1/files/{file_id}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (_, bin) = call(&t.app, Method::GET, "/api/v1/recycle-bin", None).await;
    assert!(bin.as_array().unwrap().is_empty());
}

#[tokio::test]
async fn recycle_bin_cleanup_honours_retention() {
    let t = test_app().await;
    let file_id = seed_file(&t, "Book", &t.root.join("Book.pdf"), "h1").await;
    call(
        &t.app,
        Method::POST,
        &format!("/api/v1/files/{file_id}/recycle"),
        None,
    )
    .await;

    let (status, kept) = call(&t.app, Method::POST, "/api/v1/recycle-bin/cleanup", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(kept["purged"], 0);
    assert_eq!(kept["retention_days"], 30);

    let (_, purged) = call(
        &t.app,
        Method::POST,
        "/api/v1/recycle-bin/cleanup?retention_days=0",
        None,
    )
    .await;
    assert_eq!(purged["purged"], 1);
}

#[tokio::test]
async fn duplicates_exact_and_near() {
    let t = test_app().await;
    seed_file(&t, "Dune", &t.root.join("a/Dune.pdf"), "same").await;
    seed_file(&t, "Dune (copy)", &t.root.join("b/Dune.pdf"), "same").await;
    seed_file(&t, "Neuromancer", &t.root.join("c/N.pdf"), "other").await;
    seed_file(&t, "The Neuromancer", &t.root.join("d/N.pdf"), "other2").await;

    let (status, groups) = call(&t.app, Method::GET, "/api/v1/duplicates", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(groups.as_array().unwrap().len(), 1);
    assert_eq!(groups[0]["sha256"], "same");
    assert_eq!(groups[0]["files"].as_array().unwrap().len(), 2);

    let (status, near) = call(&t.app, Method::GET, "/api/v1/duplicates/near", None).await;
    assert_eq!(status, StatusCode::OK);
    let pairs = near.as_array().unwrap();
    assert_eq!(pairs.len(), 1, "{near}");
    assert_eq!(pairs[0]["similarity"], 1.0);

    let (status, _) = call(
        &t.app,
        Method::GET,
        "/api/v1/duplicates/near?threshold=2",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn monitor_work_appears_in_wanted() {
    let t = test_app().await;
    let (status, created) = call(
        &t.app,
        Method::POST,
        "/api/v1/works",
        Some(work_body("Dune", "book")),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let id = created["id"].as_str().unwrap();
    assert_eq!(created["monitored"], false);

    let (status, wanted) = call(&t.app, Method::GET, "/api/v1/wanted", None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(wanted.as_array().unwrap().is_empty());

    let (status, updated) = call(
        &t.app,
        Method::POST,
        &format!("/api/v1/works/{id}/monitor"),
        Some(json!({"monitored": true})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["monitored"], true);

    let (status, wanted) = call(&t.app, Method::GET, "/api/v1/wanted", None).await;
    assert_eq!(status, StatusCode::OK);
    let ids: Vec<&str> = wanted
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w["id"].as_str().unwrap())
        .collect();
    assert!(ids.contains(&id));

    let (status, queue) = call(&t.app, Method::GET, "/api/v1/queue", None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(queue.as_array().unwrap().is_empty());

    let (status, err) = call(
        &t.app,
        Method::POST,
        &format!("/api/v1/works/{id}/search"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(err["error"]["code"], "BAD_REQUEST");
}
