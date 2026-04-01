use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use curatarr_core::error::DbError;
use curatarr_core::traits::repository::Repository;
use curatarr_core::types::author::{Author, AuthorFilter, AuthorUpdate, NewAuthor};
use curatarr_core::types::collection::{Collection, CollectionUpdate, NewCollection};
use curatarr_core::types::edition::{Edition, EditionFilter, EditionUpdate, NewEdition};
use curatarr_core::types::enums::AuthorRole;
use curatarr_core::types::file::{FileFilter, LibraryFile, LibraryFileUpdate, NewLibraryFile};
use curatarr_core::types::id::*;
use curatarr_core::types::publisher::{NewPublisher, Publisher, PublisherFilter, PublisherUpdate};
use curatarr_core::types::series::{
    NewSeries, NewSeriesEntry, Series, SeriesEntry, SeriesFilter, SeriesUpdate,
};
use curatarr_core::types::tag::{NewTag, Tag, TagUpdate};
use curatarr_core::types::work::{NewWork, Work, WorkFilter, WorkUpdate};
use curatarr_core::types::{Page, Pagination};

pub struct SqliteRepository {
    pool: SqlitePool,
}

impl SqliteRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

fn now_iso() -> String {
    Utc::now().format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string()
}

fn parse_dt(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .or_else(|_| {
            chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.fZ")
                .map(|ndt| ndt.and_utc())
        })
        .unwrap_or_default()
}

fn parse_opt_dt(s: Option<&str>) -> Option<DateTime<Utc>> {
    s.map(parse_dt)
}

fn parse_date(s: Option<&str>) -> Option<NaiveDate> {
    s.and_then(|v| NaiveDate::parse_from_str(v, "%Y-%m-%d").ok())
}

fn parse_uuid(s: &str) -> Uuid {
    Uuid::parse_str(s).unwrap_or_default()
}

fn ser_enum<T: serde::Serialize>(val: &T) -> String {
    let json = serde_json::to_string(val).unwrap_or_default();
    json.trim_matches('"').to_string()
}

fn de_enum<T: serde::de::DeserializeOwned>(s: &str) -> T {
    serde_json::from_str(&format!("\"{s}\"")).unwrap_or_else(|_| {
        serde_json::from_str(&format!("\"{}\"", s.to_lowercase()))
            .expect("failed to deserialize enum")
    })
}

fn offset_for(page: &Pagination) -> i64 {
    i64::from(page.page.saturating_sub(1)) * i64::from(page.per_page)
}

fn work_from_row(row: &sqlx::sqlite::SqliteRow) -> Work {
    let warnings_json: String = row.get("content_warnings");
    let content_warnings: Vec<String> = serde_json::from_str(&warnings_json).unwrap_or_default();

    Work {
        id: WorkId::from_uuid(parse_uuid(row.get("id"))),
        title: row.get("title"),
        sort_title: row.get("sort_title"),
        original_language: row.get("original_language"),
        original_pub_date: parse_date(row.get("original_pub_date")),
        description: row.get("description"),
        description_html: row.get("description_html"),
        content_type: de_enum(row.get("content_type")),
        age_rating: row
            .get::<Option<String>, _>("age_rating")
            .as_deref()
            .map(de_enum),
        content_warnings,
        average_rating: row.get("average_rating"),
        user_rating: row.get("user_rating"),
        user_review: row.get("user_review"),
        read_status: de_enum(row.get("read_status")),
        user_notes: row.get("user_notes"),
        created_at: parse_dt(row.get("created_at")),
        updated_at: parse_dt(row.get("updated_at")),
    }
}

fn edition_from_row(row: &sqlx::sqlite::SqliteRow) -> Edition {
    Edition {
        id: EditionId::from_uuid(parse_uuid(row.get("id"))),
        work_id: WorkId::from_uuid(parse_uuid(row.get("work_id"))),
        isbn13: row
            .get::<Option<String>, _>("isbn13")
            .and_then(|s| s.try_into().ok()),
        isbn10: row
            .get::<Option<String>, _>("isbn10")
            .and_then(|s| s.try_into().ok()),
        asin: row
            .get::<Option<String>, _>("asin")
            .and_then(|s| s.try_into().ok()),
        publisher_id: row
            .get::<Option<String>, _>("publisher_id")
            .map(|s| PublisherId::from_uuid(parse_uuid(&s))),
        imprint: row.get("imprint"),
        publication_date: parse_date(row.get("publication_date")),
        edition_number: row
            .get::<Option<i32>, _>("edition_number")
            .map(|v| v as u32),
        format: de_enum(row.get("format")),
        page_count: row.get::<Option<i32>, _>("page_count").map(|v| v as u32),
        word_count: row.get::<Option<i64>, _>("word_count").map(|v| v as u64),
        language: row.get("language"),
        translator: row.get("translator"),
        cover_path: row.get("cover_path"),
        created_at: parse_dt(row.get("created_at")),
        updated_at: parse_dt(row.get("updated_at")),
    }
}

fn author_from_row(row: &sqlx::sqlite::SqliteRow) -> Author {
    Author {
        id: AuthorId::from_uuid(parse_uuid(row.get("id"))),
        name: row.get("name"),
        sort_name: row.get("sort_name"),
        birth_date: parse_date(row.get("birth_date")),
        death_date: parse_date(row.get("death_date")),
        nationality: row.get("nationality"),
        biography: row.get("biography"),
        biography_html: row.get("biography_html"),
        photo_path: row.get("photo_path"),
        external_ids: vec![],
        created_at: parse_dt(row.get("created_at")),
        updated_at: parse_dt(row.get("updated_at")),
    }
}

fn series_from_row(row: &sqlx::sqlite::SqliteRow) -> Series {
    Series {
        id: SeriesId::from_uuid(parse_uuid(row.get("id"))),
        title: row.get("title"),
        sort_title: row.get("sort_title"),
        description: row.get("description"),
        series_type: de_enum(row.get("series_type")),
        reading_order: de_enum(row.get("reading_order")),
        volume_count: row.get::<Option<i32>, _>("volume_count").map(|v| v as u32),
        expected_volume_count: row
            .get::<Option<i32>, _>("expected_volume_count")
            .map(|v| v as u32),
        external_ids: vec![],
        created_at: parse_dt(row.get("created_at")),
        updated_at: parse_dt(row.get("updated_at")),
    }
}

fn publisher_from_row(row: &sqlx::sqlite::SqliteRow) -> Publisher {
    Publisher {
        id: PublisherId::from_uuid(parse_uuid(row.get("id"))),
        name: row.get("name"),
        sort_name: row.get("sort_name"),
        imprint: row.get("imprint"),
        parent_publisher_id: row
            .get::<Option<String>, _>("parent_publisher_id")
            .map(|s| PublisherId::from_uuid(parse_uuid(&s))),
        country: row.get("country"),
        founding_year: row.get("founding_year"),
        created_at: parse_dt(row.get("created_at")),
        updated_at: parse_dt(row.get("updated_at")),
    }
}

fn collection_from_row(row: &sqlx::sqlite::SqliteRow) -> Collection {
    Collection {
        id: CollectionId::from_uuid(parse_uuid(row.get("id"))),
        name: row.get("name"),
        description: row.get("description"),
        created_at: parse_dt(row.get("created_at")),
        updated_at: parse_dt(row.get("updated_at")),
    }
}

fn tag_from_row(row: &sqlx::sqlite::SqliteRow) -> Tag {
    Tag {
        id: TagId::from_uuid(parse_uuid(row.get("id"))),
        name: row.get("name"),
        parent_id: row
            .get::<Option<String>, _>("parent_id")
            .map(|s| TagId::from_uuid(parse_uuid(&s))),
        created_at: parse_dt(row.get("created_at")),
        updated_at: parse_dt(row.get("updated_at")),
    }
}

fn file_from_row(row: &sqlx::sqlite::SqliteRow) -> LibraryFile {
    LibraryFile {
        id: FileId::from_uuid(parse_uuid(row.get("id"))),
        edition_id: EditionId::from_uuid(parse_uuid(row.get("edition_id"))),
        path: row.get("path"),
        format: de_enum(row.get("format")),
        size_bytes: row.get::<i64, _>("size_bytes") as u64,
        sha256: row.get("sha256"),
        import_date: parse_dt(row.get("import_date")),
        deleted_at: parse_opt_dt(row.get("deleted_at")),
        created_at: parse_dt(row.get("created_at")),
        updated_at: parse_dt(row.get("updated_at")),
    }
}

fn series_entry_from_row(row: &sqlx::sqlite::SqliteRow) -> SeriesEntry {
    SeriesEntry {
        id: SeriesEntryId::from_uuid(parse_uuid(row.get("id"))),
        series_id: SeriesId::from_uuid(parse_uuid(row.get("series_id"))),
        work_id: WorkId::from_uuid(parse_uuid(row.get("work_id"))),
        position: row.get("position"),
        arc: row.get("arc"),
    }
}

#[async_trait]
impl Repository for SqliteRepository {
    // --- Works ---

    async fn create_work(&self, work: &NewWork) -> Result<Work, DbError> {
        let id = WorkId::new();
        let now = now_iso();
        let warnings = serde_json::to_string(&work.content_warnings).unwrap_or_default();

        sqlx::query(
            "INSERT INTO works (id, title, sort_title, original_language, original_pub_date,
             description, description_html, content_type, age_rating, content_warnings,
             read_status, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id.to_string())
        .bind(&work.title)
        .bind(&work.sort_title)
        .bind(&work.original_language)
        .bind(work.original_pub_date.map(|d| d.to_string()))
        .bind(&work.description)
        .bind(&work.description_html)
        .bind(ser_enum(&work.content_type))
        .bind(work.age_rating.map(|r| ser_enum(&r)))
        .bind(&warnings)
        .bind(ser_enum(&work.read_status))
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await
        .map_err(|e| DbError::Internal(Box::new(e)))?;

        self.get_work(id).await?.ok_or(DbError::NotFound {
            entity: "work",
            id: id.to_string(),
        })
    }

    async fn get_work(&self, id: WorkId) -> Result<Option<Work>, DbError> {
        let row = sqlx::query("SELECT * FROM works WHERE id = ?")
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DbError::Internal(Box::new(e)))?;

        Ok(row.as_ref().map(work_from_row))
    }

    async fn list_works(
        &self,
        filter: &WorkFilter,
        page: &Pagination,
    ) -> Result<Page<Work>, DbError> {
        let mut where_clauses = Vec::new();
        let mut binds: Vec<String> = Vec::new();

        if let Some(ct) = &filter.content_type {
            where_clauses.push("content_type = ?");
            binds.push(ser_enum(ct));
        }
        if let Some(rs) = &filter.read_status {
            where_clauses.push("read_status = ?");
            binds.push(ser_enum(rs));
        }
        if let Some(ar) = &filter.age_rating {
            where_clauses.push("age_rating = ?");
            binds.push(ser_enum(ar));
        }
        if let Some(title) = &filter.title_contains {
            where_clauses.push("title LIKE ?");
            binds.push(format!("%{title}%"));
        }
        if let Some(lang) = &filter.language {
            where_clauses.push("original_language = ?");
            binds.push(lang.clone()); // clone: building dynamic query bind list
        }

        let where_sql = if where_clauses.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", where_clauses.join(" AND "))
        };

        let count_sql = format!("SELECT COUNT(*) as cnt FROM works {where_sql}");
        let mut count_q = sqlx::query(&count_sql);
        for b in &binds {
            count_q = count_q.bind(b);
        }
        let total: i64 = count_q
            .fetch_one(&self.pool)
            .await
            .map_err(|e| DbError::Internal(Box::new(e)))?
            .get("cnt");

        let select_sql =
            format!("SELECT * FROM works {where_sql} ORDER BY sort_title ASC LIMIT ? OFFSET ?");
        let mut select_q = sqlx::query(&select_sql);
        for b in &binds {
            select_q = select_q.bind(b);
        }
        select_q = select_q
            .bind(i64::from(page.per_page))
            .bind(offset_for(page));

        let rows = select_q
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DbError::Internal(Box::new(e)))?;

        Ok(Page {
            items: rows.iter().map(work_from_row).collect(),
            total: total as u64,
            page: page.page,
            per_page: page.per_page,
        })
    }

    async fn update_work(&self, id: WorkId, update: &WorkUpdate) -> Result<Work, DbError> {
        let mut sets = vec!["updated_at = ?".to_string()];
        let mut binds: Vec<String> = vec![now_iso()];

        if let Some(title) = &update.title {
            sets.push("title = ?".into());
            binds.push(title.clone()); // clone: building dynamic query bind list
        }
        if let Some(sort_title) = &update.sort_title {
            sets.push("sort_title = ?".into());
            binds.push(sort_title.clone()); // clone: building dynamic query bind list
        }
        if let Some(ct) = &update.content_type {
            sets.push("content_type = ?".into());
            binds.push(ser_enum(ct));
        }
        if let Some(rs) = &update.read_status {
            sets.push("read_status = ?".into());
            binds.push(ser_enum(rs));
        }
        if let Some(warnings) = &update.content_warnings {
            sets.push("content_warnings = ?".into());
            binds.push(serde_json::to_string(warnings).unwrap_or_default());
        }

        let sql = format!("UPDATE works SET {} WHERE id = ?", sets.join(", "));
        let mut q = sqlx::query(&sql);
        for b in &binds {
            q = q.bind(b);
        }
        q = q.bind(id.to_string());

        q.execute(&self.pool)
            .await
            .map_err(|e| DbError::Internal(Box::new(e)))?;

        self.get_work(id).await?.ok_or(DbError::NotFound {
            entity: "work",
            id: id.to_string(),
        })
    }

    async fn delete_work(&self, id: WorkId) -> Result<(), DbError> {
        sqlx::query("DELETE FROM works WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| DbError::Internal(Box::new(e)))?;
        Ok(())
    }

    // --- Editions ---

    async fn create_edition(&self, edition: &NewEdition) -> Result<Edition, DbError> {
        let id = EditionId::new();
        let now = now_iso();

        sqlx::query(
            "INSERT INTO editions (id, work_id, isbn13, isbn10, asin, publisher_id, imprint,
             publication_date, edition_number, format, page_count, word_count, language,
             translator, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id.to_string())
        .bind(edition.work_id.to_string())
        .bind(edition.isbn13.as_ref().map(|i| i.as_str().to_string()))
        .bind(edition.isbn10.as_ref().map(|i| i.as_str().to_string()))
        .bind(edition.asin.as_ref().map(|a| a.as_str().to_string()))
        .bind(edition.publisher_id.map(|p| p.to_string()))
        .bind(&edition.imprint)
        .bind(edition.publication_date.map(|d| d.to_string()))
        .bind(edition.edition_number.map(|n| n as i32))
        .bind(ser_enum(&edition.format))
        .bind(edition.page_count.map(|n| n as i32))
        .bind(edition.word_count.map(|n| n as i64))
        .bind(&edition.language)
        .bind(&edition.translator)
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await
        .map_err(|e| DbError::Internal(Box::new(e)))?;

        self.get_edition(id).await?.ok_or(DbError::NotFound {
            entity: "edition",
            id: id.to_string(),
        })
    }

    async fn get_edition(&self, id: EditionId) -> Result<Option<Edition>, DbError> {
        let row = sqlx::query("SELECT * FROM editions WHERE id = ?")
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DbError::Internal(Box::new(e)))?;

        Ok(row.as_ref().map(edition_from_row))
    }

    async fn list_editions(
        &self,
        filter: &EditionFilter,
        page: &Pagination,
    ) -> Result<Page<Edition>, DbError> {
        let mut wheres = Vec::new();
        let mut binds: Vec<String> = Vec::new();

        if let Some(wid) = &filter.work_id {
            wheres.push("work_id = ?");
            binds.push(wid.to_string());
        }
        if let Some(fmt) = &filter.format {
            wheres.push("format = ?");
            binds.push(ser_enum(fmt));
        }

        let where_sql = if wheres.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", wheres.join(" AND "))
        };

        let total = fetch_count(&self.pool, "editions", &where_sql, &binds).await?;
        let rows = fetch_page(&self.pool, "editions", &where_sql, &binds, page).await?;

        Ok(Page {
            items: rows.iter().map(edition_from_row).collect(),
            total,
            page: page.page,
            per_page: page.per_page,
        })
    }

    async fn update_edition(
        &self,
        id: EditionId,
        update: &EditionUpdate,
    ) -> Result<Edition, DbError> {
        let mut sets = vec!["updated_at = ?".to_string()];
        let mut binds: Vec<String> = vec![now_iso()];

        if let Some(fmt) = &update.format {
            sets.push("format = ?".into());
            binds.push(ser_enum(fmt));
        }

        let sql = format!("UPDATE editions SET {} WHERE id = ?", sets.join(", "));
        let mut q = sqlx::query(&sql);
        for b in &binds {
            q = q.bind(b);
        }
        q = q.bind(id.to_string());
        q.execute(&self.pool)
            .await
            .map_err(|e| DbError::Internal(Box::new(e)))?;

        self.get_edition(id).await?.ok_or(DbError::NotFound {
            entity: "edition",
            id: id.to_string(),
        })
    }

    async fn delete_edition(&self, id: EditionId) -> Result<(), DbError> {
        sqlx::query("DELETE FROM editions WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| DbError::Internal(Box::new(e)))?;
        Ok(())
    }

    // --- Authors ---

    async fn create_author(&self, author: &NewAuthor) -> Result<Author, DbError> {
        let id = AuthorId::new();
        let now = now_iso();

        sqlx::query(
            "INSERT INTO authors (id, name, sort_name, birth_date, death_date, nationality,
             biography, biography_html, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id.to_string())
        .bind(&author.name)
        .bind(&author.sort_name)
        .bind(author.birth_date.map(|d| d.to_string()))
        .bind(author.death_date.map(|d| d.to_string()))
        .bind(&author.nationality)
        .bind(&author.biography)
        .bind(&author.biography_html)
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await
        .map_err(|e| DbError::Internal(Box::new(e)))?;

        self.get_author(id).await?.ok_or(DbError::NotFound {
            entity: "author",
            id: id.to_string(),
        })
    }

    async fn get_author(&self, id: AuthorId) -> Result<Option<Author>, DbError> {
        let row = sqlx::query("SELECT * FROM authors WHERE id = ?")
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DbError::Internal(Box::new(e)))?;

        Ok(row.as_ref().map(author_from_row))
    }

    async fn list_authors(
        &self,
        filter: &AuthorFilter,
        page: &Pagination,
    ) -> Result<Page<Author>, DbError> {
        let mut wheres = Vec::new();
        let mut binds: Vec<String> = Vec::new();

        if let Some(name) = &filter.name_contains {
            wheres.push("name LIKE ?");
            binds.push(format!("%{name}%"));
        }

        let where_sql = if wheres.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", wheres.join(" AND "))
        };

        let total = fetch_count(&self.pool, "authors", &where_sql, &binds).await?;
        let rows = fetch_page(&self.pool, "authors", &where_sql, &binds, page).await?;

        Ok(Page {
            items: rows.iter().map(author_from_row).collect(),
            total,
            page: page.page,
            per_page: page.per_page,
        })
    }

    async fn update_author(&self, id: AuthorId, update: &AuthorUpdate) -> Result<Author, DbError> {
        let mut sets = vec!["updated_at = ?".to_string()];
        let mut binds: Vec<String> = vec![now_iso()];

        if let Some(name) = &update.name {
            sets.push("name = ?".into());
            binds.push(name.clone()); // clone: dynamic bind list
        }
        if let Some(sort_name) = &update.sort_name {
            sets.push("sort_name = ?".into());
            binds.push(sort_name.clone()); // clone: dynamic bind list
        }

        let sql = format!("UPDATE authors SET {} WHERE id = ?", sets.join(", "));
        let mut q = sqlx::query(&sql);
        for b in &binds {
            q = q.bind(b);
        }
        q = q.bind(id.to_string());
        q.execute(&self.pool)
            .await
            .map_err(|e| DbError::Internal(Box::new(e)))?;

        self.get_author(id).await?.ok_or(DbError::NotFound {
            entity: "author",
            id: id.to_string(),
        })
    }

    async fn delete_author(&self, id: AuthorId) -> Result<(), DbError> {
        sqlx::query("DELETE FROM authors WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| DbError::Internal(Box::new(e)))?;
        Ok(())
    }

    // --- Work-Author ---

    async fn link_work_author(
        &self,
        work_id: WorkId,
        author_id: AuthorId,
        role: AuthorRole,
    ) -> Result<(), DbError> {
        sqlx::query(
            "INSERT OR IGNORE INTO work_authors (work_id, author_id, role) VALUES (?, ?, ?)",
        )
        .bind(work_id.to_string())
        .bind(author_id.to_string())
        .bind(ser_enum(&role))
        .execute(&self.pool)
        .await
        .map_err(|e| DbError::Internal(Box::new(e)))?;
        Ok(())
    }

    async fn unlink_work_author(
        &self,
        work_id: WorkId,
        author_id: AuthorId,
    ) -> Result<(), DbError> {
        sqlx::query("DELETE FROM work_authors WHERE work_id = ? AND author_id = ?")
            .bind(work_id.to_string())
            .bind(author_id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| DbError::Internal(Box::new(e)))?;
        Ok(())
    }

    // --- Series ---

    async fn create_series(&self, series: &NewSeries) -> Result<Series, DbError> {
        let id = SeriesId::new();
        let now = now_iso();

        sqlx::query(
            "INSERT INTO series (id, title, sort_title, description, series_type, reading_order,
             volume_count, expected_volume_count, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id.to_string())
        .bind(&series.title)
        .bind(&series.sort_title)
        .bind(&series.description)
        .bind(ser_enum(&series.series_type))
        .bind(ser_enum(&series.reading_order))
        .bind(series.volume_count.map(|n| n as i32))
        .bind(series.expected_volume_count.map(|n| n as i32))
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await
        .map_err(|e| DbError::Internal(Box::new(e)))?;

        self.get_series(id).await?.ok_or(DbError::NotFound {
            entity: "series",
            id: id.to_string(),
        })
    }

    async fn get_series(&self, id: SeriesId) -> Result<Option<Series>, DbError> {
        let row = sqlx::query("SELECT * FROM series WHERE id = ?")
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DbError::Internal(Box::new(e)))?;

        Ok(row.as_ref().map(series_from_row))
    }

    async fn list_series(
        &self,
        filter: &SeriesFilter,
        page: &Pagination,
    ) -> Result<Page<Series>, DbError> {
        let mut wheres = Vec::new();
        let mut binds: Vec<String> = Vec::new();

        if let Some(title) = &filter.title_contains {
            wheres.push("title LIKE ?");
            binds.push(format!("%{title}%"));
        }
        if let Some(st) = &filter.series_type {
            wheres.push("series_type = ?");
            binds.push(ser_enum(st));
        }

        let where_sql = if wheres.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", wheres.join(" AND "))
        };

        let total = fetch_count(&self.pool, "series", &where_sql, &binds).await?;
        let rows = fetch_page(&self.pool, "series", &where_sql, &binds, page).await?;

        Ok(Page {
            items: rows.iter().map(series_from_row).collect(),
            total,
            page: page.page,
            per_page: page.per_page,
        })
    }

    async fn update_series(&self, id: SeriesId, update: &SeriesUpdate) -> Result<Series, DbError> {
        let mut sets = vec!["updated_at = ?".to_string()];
        let mut binds: Vec<String> = vec![now_iso()];

        if let Some(title) = &update.title {
            sets.push("title = ?".into());
            binds.push(title.clone()); // clone: dynamic bind list
        }

        let sql = format!("UPDATE series SET {} WHERE id = ?", sets.join(", "));
        let mut q = sqlx::query(&sql);
        for b in &binds {
            q = q.bind(b);
        }
        q = q.bind(id.to_string());
        q.execute(&self.pool)
            .await
            .map_err(|e| DbError::Internal(Box::new(e)))?;

        self.get_series(id).await?.ok_or(DbError::NotFound {
            entity: "series",
            id: id.to_string(),
        })
    }

    async fn delete_series(&self, id: SeriesId) -> Result<(), DbError> {
        sqlx::query("DELETE FROM series WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| DbError::Internal(Box::new(e)))?;
        Ok(())
    }

    // --- Series Entries ---

    async fn create_series_entry(&self, entry: &NewSeriesEntry) -> Result<SeriesEntry, DbError> {
        let id = SeriesEntryId::new();

        sqlx::query(
            "INSERT INTO series_entries (id, series_id, work_id, position, arc) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(id.to_string())
        .bind(entry.series_id.to_string())
        .bind(entry.work_id.to_string())
        .bind(entry.position)
        .bind(&entry.arc)
        .execute(&self.pool)
        .await
        .map_err(|e| DbError::Internal(Box::new(e)))?;

        Ok(SeriesEntry {
            id,
            series_id: entry.series_id,
            work_id: entry.work_id,
            position: entry.position,
            arc: entry.arc.clone(), // clone: constructing return value from borrowed input
        })
    }

    async fn delete_series_entry(&self, id: SeriesEntryId) -> Result<(), DbError> {
        sqlx::query("DELETE FROM series_entries WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| DbError::Internal(Box::new(e)))?;
        Ok(())
    }

    async fn list_series_entries(&self, series_id: SeriesId) -> Result<Vec<SeriesEntry>, DbError> {
        let rows =
            sqlx::query("SELECT * FROM series_entries WHERE series_id = ? ORDER BY position ASC")
                .bind(series_id.to_string())
                .fetch_all(&self.pool)
                .await
                .map_err(|e| DbError::Internal(Box::new(e)))?;

        Ok(rows.iter().map(series_entry_from_row).collect())
    }

    // --- Publishers ---

    async fn create_publisher(&self, publisher: &NewPublisher) -> Result<Publisher, DbError> {
        let id = PublisherId::new();
        let now = now_iso();

        sqlx::query(
            "INSERT INTO publishers (id, name, sort_name, imprint, parent_publisher_id, country,
             founding_year, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id.to_string())
        .bind(&publisher.name)
        .bind(&publisher.sort_name)
        .bind(&publisher.imprint)
        .bind(publisher.parent_publisher_id.map(|p| p.to_string()))
        .bind(&publisher.country)
        .bind(publisher.founding_year)
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await
        .map_err(|e| DbError::Internal(Box::new(e)))?;

        self.get_publisher(id).await?.ok_or(DbError::NotFound {
            entity: "publisher",
            id: id.to_string(),
        })
    }

    async fn get_publisher(&self, id: PublisherId) -> Result<Option<Publisher>, DbError> {
        let row = sqlx::query("SELECT * FROM publishers WHERE id = ?")
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DbError::Internal(Box::new(e)))?;

        Ok(row.as_ref().map(publisher_from_row))
    }

    async fn list_publishers(
        &self,
        filter: &PublisherFilter,
        page: &Pagination,
    ) -> Result<Page<Publisher>, DbError> {
        let mut wheres = Vec::new();
        let mut binds: Vec<String> = Vec::new();

        if let Some(name) = &filter.name_contains {
            wheres.push("name LIKE ?");
            binds.push(format!("%{name}%"));
        }

        let where_sql = if wheres.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", wheres.join(" AND "))
        };

        let total = fetch_count(&self.pool, "publishers", &where_sql, &binds).await?;
        let rows = fetch_page(&self.pool, "publishers", &where_sql, &binds, page).await?;

        Ok(Page {
            items: rows.iter().map(publisher_from_row).collect(),
            total,
            page: page.page,
            per_page: page.per_page,
        })
    }

    async fn update_publisher(
        &self,
        id: PublisherId,
        update: &PublisherUpdate,
    ) -> Result<Publisher, DbError> {
        let mut sets = vec!["updated_at = ?".to_string()];
        let mut binds: Vec<String> = vec![now_iso()];

        if let Some(name) = &update.name {
            sets.push("name = ?".into());
            binds.push(name.clone()); // clone: dynamic bind list
        }

        let sql = format!("UPDATE publishers SET {} WHERE id = ?", sets.join(", "));
        let mut q = sqlx::query(&sql);
        for b in &binds {
            q = q.bind(b);
        }
        q = q.bind(id.to_string());
        q.execute(&self.pool)
            .await
            .map_err(|e| DbError::Internal(Box::new(e)))?;

        self.get_publisher(id).await?.ok_or(DbError::NotFound {
            entity: "publisher",
            id: id.to_string(),
        })
    }

    async fn delete_publisher(&self, id: PublisherId) -> Result<(), DbError> {
        sqlx::query("DELETE FROM publishers WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| DbError::Internal(Box::new(e)))?;
        Ok(())
    }

    // --- Collections ---

    async fn create_collection(&self, collection: &NewCollection) -> Result<Collection, DbError> {
        let id = CollectionId::new();
        let now = now_iso();

        sqlx::query(
            "INSERT INTO collections (id, name, description, created_at, updated_at) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(id.to_string())
        .bind(&collection.name)
        .bind(&collection.description)
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await
        .map_err(|e| DbError::Internal(Box::new(e)))?;

        self.get_collection(id).await?.ok_or(DbError::NotFound {
            entity: "collection",
            id: id.to_string(),
        })
    }

    async fn get_collection(&self, id: CollectionId) -> Result<Option<Collection>, DbError> {
        let row = sqlx::query("SELECT * FROM collections WHERE id = ?")
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DbError::Internal(Box::new(e)))?;

        Ok(row.as_ref().map(collection_from_row))
    }

    async fn list_collections(&self, page: &Pagination) -> Result<Page<Collection>, DbError> {
        let total = fetch_count(&self.pool, "collections", "", &[]).await?;
        let rows = fetch_page(&self.pool, "collections", "", &[], page).await?;

        Ok(Page {
            items: rows.iter().map(collection_from_row).collect(),
            total,
            page: page.page,
            per_page: page.per_page,
        })
    }

    async fn update_collection(
        &self,
        id: CollectionId,
        update: &CollectionUpdate,
    ) -> Result<Collection, DbError> {
        let mut sets = vec!["updated_at = ?".to_string()];
        let mut binds: Vec<String> = vec![now_iso()];

        if let Some(name) = &update.name {
            sets.push("name = ?".into());
            binds.push(name.clone()); // clone: dynamic bind list
        }

        let sql = format!("UPDATE collections SET {} WHERE id = ?", sets.join(", "));
        let mut q = sqlx::query(&sql);
        for b in &binds {
            q = q.bind(b);
        }
        q = q.bind(id.to_string());
        q.execute(&self.pool)
            .await
            .map_err(|e| DbError::Internal(Box::new(e)))?;

        self.get_collection(id).await?.ok_or(DbError::NotFound {
            entity: "collection",
            id: id.to_string(),
        })
    }

    async fn delete_collection(&self, id: CollectionId) -> Result<(), DbError> {
        sqlx::query("DELETE FROM collections WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| DbError::Internal(Box::new(e)))?;
        Ok(())
    }

    async fn add_work_to_collection(
        &self,
        collection_id: CollectionId,
        work_id: WorkId,
    ) -> Result<(), DbError> {
        sqlx::query(
            "INSERT OR IGNORE INTO collection_works (collection_id, work_id) VALUES (?, ?)",
        )
        .bind(collection_id.to_string())
        .bind(work_id.to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| DbError::Internal(Box::new(e)))?;
        Ok(())
    }

    async fn remove_work_from_collection(
        &self,
        collection_id: CollectionId,
        work_id: WorkId,
    ) -> Result<(), DbError> {
        sqlx::query("DELETE FROM collection_works WHERE collection_id = ? AND work_id = ?")
            .bind(collection_id.to_string())
            .bind(work_id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| DbError::Internal(Box::new(e)))?;
        Ok(())
    }

    // --- Tags ---

    async fn create_tag(&self, tag: &NewTag) -> Result<Tag, DbError> {
        let id = TagId::new();
        let now = now_iso();

        sqlx::query(
            "INSERT INTO tags (id, name, parent_id, created_at, updated_at) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(id.to_string())
        .bind(&tag.name)
        .bind(tag.parent_id.map(|p| p.to_string()))
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await
        .map_err(|e| {
            if e.to_string().contains("UNIQUE") {
                DbError::Conflict(format!("tag '{}' already exists", tag.name))
            } else {
                DbError::Internal(Box::new(e))
            }
        })?;

        self.get_tag(id).await?.ok_or(DbError::NotFound {
            entity: "tag",
            id: id.to_string(),
        })
    }

    async fn get_tag(&self, id: TagId) -> Result<Option<Tag>, DbError> {
        let row = sqlx::query("SELECT * FROM tags WHERE id = ?")
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DbError::Internal(Box::new(e)))?;

        Ok(row.as_ref().map(tag_from_row))
    }

    async fn list_tags(&self) -> Result<Vec<Tag>, DbError> {
        let rows = sqlx::query("SELECT * FROM tags ORDER BY name ASC")
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DbError::Internal(Box::new(e)))?;

        Ok(rows.iter().map(tag_from_row).collect())
    }

    async fn update_tag(&self, id: TagId, update: &TagUpdate) -> Result<Tag, DbError> {
        let mut sets = vec!["updated_at = ?".to_string()];
        let mut binds: Vec<String> = vec![now_iso()];

        if let Some(name) = &update.name {
            sets.push("name = ?".into());
            binds.push(name.clone()); // clone: dynamic bind list
        }

        let sql = format!("UPDATE tags SET {} WHERE id = ?", sets.join(", "));
        let mut q = sqlx::query(&sql);
        for b in &binds {
            q = q.bind(b);
        }
        q = q.bind(id.to_string());
        q.execute(&self.pool)
            .await
            .map_err(|e| DbError::Internal(Box::new(e)))?;

        self.get_tag(id).await?.ok_or(DbError::NotFound {
            entity: "tag",
            id: id.to_string(),
        })
    }

    async fn delete_tag(&self, id: TagId) -> Result<(), DbError> {
        sqlx::query("DELETE FROM tags WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| DbError::Internal(Box::new(e)))?;
        Ok(())
    }

    async fn tag_work(&self, work_id: WorkId, tag_id: TagId) -> Result<(), DbError> {
        sqlx::query("INSERT OR IGNORE INTO work_tags (work_id, tag_id) VALUES (?, ?)")
            .bind(work_id.to_string())
            .bind(tag_id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| DbError::Internal(Box::new(e)))?;
        Ok(())
    }

    async fn untag_work(&self, work_id: WorkId, tag_id: TagId) -> Result<(), DbError> {
        sqlx::query("DELETE FROM work_tags WHERE work_id = ? AND tag_id = ?")
            .bind(work_id.to_string())
            .bind(tag_id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| DbError::Internal(Box::new(e)))?;
        Ok(())
    }

    // --- Files ---

    async fn create_file(&self, file: &NewLibraryFile) -> Result<LibraryFile, DbError> {
        let id = FileId::new();
        let now = now_iso();

        sqlx::query(
            "INSERT INTO files (id, edition_id, path, format, size_bytes, sha256,
             import_date, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id.to_string())
        .bind(file.edition_id.to_string())
        .bind(&file.path)
        .bind(ser_enum(&file.format))
        .bind(file.size_bytes as i64)
        .bind(&file.sha256)
        .bind(&now)
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await
        .map_err(|e| DbError::Internal(Box::new(e)))?;

        self.get_file(id).await?.ok_or(DbError::NotFound {
            entity: "file",
            id: id.to_string(),
        })
    }

    async fn get_file(&self, id: FileId) -> Result<Option<LibraryFile>, DbError> {
        let row = sqlx::query("SELECT * FROM files WHERE id = ?")
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DbError::Internal(Box::new(e)))?;

        Ok(row.as_ref().map(file_from_row))
    }

    async fn list_files(
        &self,
        filter: &FileFilter,
        page: &Pagination,
    ) -> Result<Page<LibraryFile>, DbError> {
        let mut wheres = Vec::new();
        let mut binds: Vec<String> = Vec::new();

        if let Some(eid) = &filter.edition_id {
            wheres.push("edition_id = ?");
            binds.push(eid.to_string());
        }
        if let Some(fmt) = &filter.format {
            wheres.push("format = ?");
            binds.push(ser_enum(fmt));
        }
        if !filter.include_deleted {
            wheres.push("deleted_at IS NULL");
        }

        let where_sql = if wheres.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", wheres.join(" AND "))
        };

        let total = fetch_count(&self.pool, "files", &where_sql, &binds).await?;
        let rows = fetch_page(&self.pool, "files", &where_sql, &binds, page).await?;

        Ok(Page {
            items: rows.iter().map(file_from_row).collect(),
            total,
            page: page.page,
            per_page: page.per_page,
        })
    }

    async fn update_file(
        &self,
        id: FileId,
        update: &LibraryFileUpdate,
    ) -> Result<LibraryFile, DbError> {
        let mut sets = vec!["updated_at = ?".to_string()];
        let mut binds: Vec<String> = vec![now_iso()];

        if let Some(path) = &update.path {
            sets.push("path = ?".into());
            binds.push(path.clone()); // clone: dynamic bind list
        }

        let sql = format!("UPDATE files SET {} WHERE id = ?", sets.join(", "));
        let mut q = sqlx::query(&sql);
        for b in &binds {
            q = q.bind(b);
        }
        q = q.bind(id.to_string());
        q.execute(&self.pool)
            .await
            .map_err(|e| DbError::Internal(Box::new(e)))?;

        self.get_file(id).await?.ok_or(DbError::NotFound {
            entity: "file",
            id: id.to_string(),
        })
    }

    async fn delete_file(&self, id: FileId) -> Result<(), DbError> {
        sqlx::query("DELETE FROM files WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| DbError::Internal(Box::new(e)))?;
        Ok(())
    }

    async fn find_file_by_hash(&self, sha256: &str) -> Result<Option<LibraryFile>, DbError> {
        let row = sqlx::query("SELECT * FROM files WHERE sha256 = ? AND deleted_at IS NULL")
            .bind(sha256)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| DbError::Internal(Box::new(e)))?;

        Ok(row.as_ref().map(file_from_row))
    }
}

async fn fetch_count(
    pool: &SqlitePool,
    table: &str,
    where_sql: &str,
    binds: &[String],
) -> Result<u64, DbError> {
    let sql = format!("SELECT COUNT(*) as cnt FROM {table} {where_sql}");
    let mut q = sqlx::query(&sql);
    for b in binds {
        q = q.bind(b);
    }
    let row = q
        .fetch_one(pool)
        .await
        .map_err(|e| DbError::Internal(Box::new(e)))?;
    let cnt: i64 = row.get("cnt");
    Ok(cnt as u64)
}

async fn fetch_page(
    pool: &SqlitePool,
    table: &str,
    where_sql: &str,
    binds: &[String],
    page: &Pagination,
) -> Result<Vec<sqlx::sqlite::SqliteRow>, DbError> {
    let sql = format!("SELECT * FROM {table} {where_sql} LIMIT ? OFFSET ?");
    let mut q = sqlx::query(&sql);
    for b in binds {
        q = q.bind(b);
    }
    q = q.bind(i64::from(page.per_page)).bind(offset_for(page));
    q.fetch_all(pool)
        .await
        .map_err(|e| DbError::Internal(Box::new(e)))
}
