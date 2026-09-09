use dioxus::prelude::*;

#[component]
fn Nav() -> Element {
    rsx! {
        header { class: "bar",
            a { href: "/", "Library" }
            a { href: "/wanted", "Wanted" }
            a { href: "/queue", "Queue" }
            a { href: "/settings", "Settings" }
        }
    }
}

#[component]
fn Chrome(title: String, children: Element) -> Element {
    rsx! {
        head {
            title { "{title} — curatarr" }
            meta { charset: "utf-8" }
            style { "{CSS}" }
        }
        body {
            Nav {}
            main { {children} }
            script { {SCRIPT} }
        }
    }
}

const CSS: &str = "body{font-family:sans-serif;margin:0;background:#111;color:#eee}.bar{display:flex;gap:1rem;padding:0.75rem 1rem;background:#1c1c1c}.bar a{color:#8cf;text-decoration:none}main{padding:1rem}label,input,button{display:block;margin:0.5rem 0}input,button{padding:0.4rem}";

const SCRIPT: &str = r#"
async function login(ev){
  ev.preventDefault();
  const f=ev.target;
  const r=await fetch('/api/v1/login',{method:'POST',headers:{'content-type':'application/json'},credentials:'include',body:JSON.stringify({username:f.username.value,password:f.password.value})});
  if(!r.ok){alert('login failed');return;}
  const j=await r.json();
  sessionStorage.setItem('csrf', j.csrf_token);
  location.href='/';
}
document.querySelector('form.login')?.addEventListener('submit', login);
"#;

#[component]
fn Login() -> Element {
    rsx! {
        Chrome { title: "Login".to_string(),
            h1 { "Sign in" }
            form { class: "login",
                label { "Username"
                    input { name: "username", required: true }
                }
                label { "Password"
                    input { name: "password", r#type: "password", required: true }
                }
                button { r#type: "submit", "Sign in" }
            }
        }
    }
}

#[derive(Clone, PartialEq)]
pub struct WorkCard {
    pub id: String,
    pub title: String,
    pub content_type: String,
}

#[derive(Clone, PartialEq)]
pub struct QueueCard {
    pub title: String,
    pub state: String,
}

#[derive(Clone, PartialEq, Props)]
struct ListProps {
    heading: String,
    works: Vec<WorkCard>,
}

#[component]
fn WorkList(props: ListProps) -> Element {
    rsx! {
        Chrome { title: props.heading.clone(),
            h1 { "{props.heading}" }
            ul {
                for w in props.works {
                    li {
                        a { href: "/works/{w.id}", "{w.title}" }
                        " ({w.content_type})"
                    }
                }
            }
        }
    }
}

#[derive(Clone, PartialEq, Props)]
struct QueueProps {
    items: Vec<QueueCard>,
}

#[component]
fn QueueView(props: QueueProps) -> Element {
    rsx! {
        Chrome { title: "Queue".to_string(),
            h1 { "Queue" }
            ul {
                for q in props.items {
                    li { "{q.title} — {q.state}" }
                }
            }
        }
    }
}

#[derive(Clone, PartialEq, Props)]
struct WorkProps {
    work: WorkCard,
    monitored: bool,
}

#[component]
fn WorkView(props: WorkProps) -> Element {
    let flag = if props.monitored {
        "monitored"
    } else {
        "not monitored"
    };
    rsx! {
        Chrome { title: props.work.title.clone(),
            h1 { "{props.work.title}" }
            p { "{flag}" }
            p { "Work id: {props.work.id}" }
        }
    }
}

#[derive(Clone, PartialEq, Props)]
struct SettingsProps {
    rows: Vec<(String, String)>,
}

#[component]
fn SettingsView(props: SettingsProps) -> Element {
    rsx! {
        Chrome { title: "Settings".to_string(),
            h1 { "Settings" }
            p { "Non-secret values. Secrets stay in credential files." }
            ul {
                for (k, v) in props.rows {
                    li { "{k} = {v}" }
                }
            }
        }
    }
}

pub fn login_page() -> String {
    let mut vdom = VirtualDom::new(Login);
    vdom.rebuild_in_place();
    dioxus_ssr::render(&vdom)
}

fn render_vdom(mut vdom: VirtualDom) -> String {
    vdom.rebuild_in_place();
    dioxus_ssr::render(&vdom)
}

pub fn library_page(works: &[WorkCard]) -> String {
    render_vdom(VirtualDom::new_with_props(
        WorkList,
        ListProps {
            heading: "Library".into(),
            works: works.to_vec(),
        },
    ))
}

pub fn wanted_page(works: &[WorkCard]) -> String {
    render_vdom(VirtualDom::new_with_props(
        WorkList,
        ListProps {
            heading: "Wanted".into(),
            works: works.to_vec(),
        },
    ))
}

pub fn queue_page(items: &[QueueCard]) -> String {
    render_vdom(VirtualDom::new_with_props(
        QueueView,
        QueueProps {
            items: items.to_vec(),
        },
    ))
}

pub fn work_page(work: &WorkCard, monitored: bool) -> String {
    render_vdom(VirtualDom::new_with_props(
        WorkView,
        WorkProps {
            work: work.clone(),
            monitored,
        },
    ))
}

pub fn settings_page(settings: &[(String, String)]) -> String {
    render_vdom(VirtualDom::new_with_props(
        SettingsView,
        SettingsProps {
            rows: settings.to_vec(),
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn login_page_has_password_field() {
        let html = login_page();
        assert!(html.contains("password"), "{html}");
        assert!(html.contains("Sign in"), "{html}");
    }

    #[test]
    fn library_lists_titles() {
        let html = library_page(&[WorkCard {
            id: "1".into(),
            title: "Dune".into(),
            content_type: "book".into(),
        }]);
        assert!(html.contains("Dune"), "{html}");
        assert!(html.contains("Library"), "{html}");
    }

    #[test]
    fn wanted_queue_settings_work_render() {
        assert!(wanted_page(&[]).contains("Wanted"));
        assert!(queue_page(&[]).contains("Queue"));
        assert!(settings_page(&[]).contains("Settings"));
        let html = work_page(
            &WorkCard {
                id: "x".into(),
                title: "Dune".into(),
                content_type: "book".into(),
            },
            true,
        );
        assert!(html.contains("monitored"), "{html}");
    }
}
