use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};
use axum::Router;
use chrono::{DateTime, TimeZone, Utc};
use http_body_util::BodyExt;
use magpie_server::{router, AppState, Clock};
use magpie_store::Store;
use serde_json::{json, Value};
use tokio::sync::Notify;
use tower::ServiceExt;

const TOKEN: &str = "test-token-0123456789";

struct FixedClock(DateTime<Utc>);

impl Clock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        self.0
    }
}

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 9, 28, 12, 0, 0).unwrap()
}

struct Harness {
    app: Router,
    changed: Arc<Notify>,
}

fn harness() -> Harness {
    let store = Arc::new(Mutex::new(Store::open_in_memory().unwrap()));
    let changed = Arc::new(Notify::new());
    let state = AppState::new(
        store,
        TOKEN,
        Arc::new(FixedClock(now())),
        Arc::clone(&changed),
    );
    Harness {
        app: router(state),
        changed,
    }
}

async fn call(
    app: &Router,
    method: Method,
    uri: &str,
    token: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut req = Request::builder().method(method).uri(uri);
    if let Some(t) = token {
        req = req.header(header::AUTHORIZATION, format!("Bearer {t}"));
    }
    let req = match body {
        Some(b) => req
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(b.to_string())),
        None => req.body(Body::empty()),
    }
    .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes)
            .unwrap_or(Value::String(String::from_utf8_lossy(&bytes).into()))
    };
    (status, value)
}

async fn authed(
    app: &Router,
    method: Method,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    call(app, method, uri, Some(TOKEN), body).await
}

#[tokio::test]
async fn health_needs_no_token() {
    let h = harness();
    let (status, body) = call(&h.app, Method::GET, "/v1/health", None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "ok");
}

#[tokio::test]
async fn everything_else_needs_the_token() {
    let h = harness();
    for (method, uri) in [
        (Method::GET, "/v1/events"),
        (Method::POST, "/v1/events"),
        (Method::GET, "/v1/reminders"),
    ] {
        let (status, body) = call(&h.app, method.clone(), uri, None, None).await;
        assert_eq!(
            status,
            StatusCode::UNAUTHORIZED,
            "{method} {uri} without token"
        );
        assert_eq!(body["error"]["code"], "unauthorized");

        let (status, _) = call(&h.app, method.clone(), uri, Some("wrong"), None).await;
        assert_eq!(
            status,
            StatusCode::UNAUTHORIZED,
            "{method} {uri} wrong token"
        );
    }
}

#[tokio::test]
async fn event_lifecycle() {
    let h = harness();
    let (status, created) = authed(
        &h.app,
        Method::POST,
        "/v1/events",
        Some(json!({"source":"claude-code","title":"备货完成","severity":"urgent","tags":["social"]})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(created["title"], "备货完成");
    assert_eq!(
        created["created_at"], "2026-09-28T12:00:00Z",
        "server clock, not client"
    );
    let id = created["id"].as_str().unwrap().to_owned();

    let (_, open) = authed(&h.app, Method::GET, "/v1/events?open=true", None).await;
    assert_eq!(open.as_array().unwrap().len(), 1);

    let (status, _) = authed(
        &h.app,
        Method::POST,
        &format!("/v1/events/{id}/ack"),
        Some(json!({"kind":"replied","text":"发","via":"cli"})),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, detail) = authed(&h.app, Method::GET, &format!("/v1/events/{id}"), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(detail["closed_at"], "2026-09-28T12:00:00Z");
    assert_eq!(detail["latest_ack"]["kind"]["kind"], "replied");
    assert_eq!(detail["deliveries"], json!([]));

    let (_, open) = authed(&h.app, Method::GET, "/v1/events?open=true", None).await;
    assert!(open.as_array().unwrap().is_empty());
}

#[tokio::test]
async fn create_wakes_the_runtime() {
    let h = harness();
    let woke = h.changed.notified();
    authed(
        &h.app,
        Method::POST,
        "/v1/events",
        Some(json!({"source":"cli","title":"x"})),
    )
    .await;
    tokio::time::timeout(std::time::Duration::from_secs(1), woke)
        .await
        .expect("changed notify fired");
}

#[tokio::test]
async fn invalid_event_is_422_with_reason() {
    let h = harness();
    let (status, body) = authed(
        &h.app,
        Method::POST,
        "/v1/events",
        Some(json!({"source":"cli","title":"  "})),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["error"]["code"], "invalid");
    assert_eq!(body["error"]["message"], "title must not be empty");
}

#[tokio::test]
async fn unknown_event_is_404() {
    let h = harness();
    let ghost = uuid::Uuid::new_v4();
    let (status, _) = authed(&h.app, Method::GET, &format!("/v1/events/{ghost}"), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, body) = authed(
        &h.app,
        Method::POST,
        &format!("/v1/events/{ghost}/ack"),
        Some(json!({"kind":"acknowledged","via":"cli"})),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "not_found");
}

#[tokio::test]
async fn snooze_into_the_past_is_rejected() {
    let h = harness();
    let (_, created) = authed(
        &h.app,
        Method::POST,
        "/v1/events",
        Some(json!({"source":"cli","title":"x"})),
    )
    .await;
    let id = created["id"].as_str().unwrap();
    let (status, _) = authed(
        &h.app,
        Method::POST,
        &format!("/v1/events/{id}/ack"),
        Some(json!({"kind":"snoozed","until":"2026-09-28T11:00:00Z","via":"cli"})),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn oversized_body_is_rejected() {
    let h = harness();
    let huge = "鹊".repeat(60_000); // ~180 KB of UTF-8
    let (status, _) = authed(
        &h.app,
        Method::POST,
        "/v1/events",
        Some(json!({"source":"cli","title":"x","body":huge})),
    )
    .await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn reminder_crud_and_validation() {
    let h = harness();
    let (status, r) = authed(
        &h.app,
        Method::POST,
        "/v1/reminders",
        Some(json!({
            "schedule": {"kind":"cron","expr":"50 19 * * 1,3,5"},
            "template": {"source":"reminder","title":"发小红书","severity":"urgent"}
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(r["enabled"], true);
    let id = r["id"].as_str().unwrap().to_owned();

    let (_, all) = authed(&h.app, Method::GET, "/v1/reminders", None).await;
    assert_eq!(all.as_array().unwrap().len(), 1);

    let (status, _) = authed(&h.app, Method::DELETE, &format!("/v1/reminders/{id}"), None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = authed(&h.app, Method::DELETE, &format!("/v1/reminders/{id}"), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, body) = authed(
        &h.app,
        Method::POST,
        "/v1/reminders",
        Some(json!({"schedule":{"kind":"cron","expr":"0 9 * * *"},"template":{"source":"r","title":""}})),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "bad template caught at creation: {body}"
    );
}

#[tokio::test]
async fn binds_loopback_and_serves_real_http() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let listener = magpie_server::bind_loopback(0).await.unwrap();
    let addr = listener.local_addr().unwrap();
    assert!(addr.ip().is_loopback());

    let store = Arc::new(Mutex::new(Store::open_in_memory().unwrap()));
    let state = AppState::new(
        store,
        TOKEN,
        Arc::new(FixedClock(now())),
        Arc::new(Notify::new()),
    );
    let server = tokio::spawn(magpie_server::serve(listener, state));

    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    stream
        .write_all(b"GET /v1/health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .await
        .unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).await.unwrap();
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    assert!(response.contains(r#""status":"ok""#));
    server.abort();
}

#[tokio::test]
async fn item_lifecycle_over_http() {
    let h = harness();
    let (status, todo) = authed(
        &h.app,
        Method::POST,
        "/v1/items",
        Some(json!({"title":"定价系数拍板","why":"4.0 服务页唯一硬闸门","action":"看 P1-P4 占位表选一组",
                    "area":"工作","day":"2026-09-29","estimate_min":45,"blocking":true})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{todo}");
    assert_eq!(todo["kind"], "todo");
    assert_eq!(todo["status"], "open");
    let id = todo["id"].as_str().unwrap().to_owned();

    let (_, idea) = authed(
        &h.app,
        Method::POST,
        "/v1/items",
        Some(json!({"kind":"idea","title":"做数字产品"})),
    )
    .await;
    assert_eq!(idea["day"], Value::Null);

    let (_, on_day) = authed(
        &h.app,
        Method::GET,
        "/v1/items?day=2026-09-29&status=open",
        None,
    )
    .await;
    assert_eq!(on_day.as_array().unwrap().len(), 1);
    let (_, pool) = authed(
        &h.app,
        Method::GET,
        "/v1/items?unscheduled=true&kind=idea",
        None,
    )
    .await;
    assert_eq!(pool[0]["title"], "做数字产品");

    let (status, done) = authed(
        &h.app,
        Method::PATCH,
        &format!("/v1/items/{id}"),
        Some(json!({"op":"done","actual_min":70})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        (done["status"].as_str(), done["actual_min"].as_u64()),
        (Some("done"), Some(70))
    );

    let (status, body) = authed(
        &h.app,
        Method::PATCH,
        &format!("/v1/items/{id}"),
        Some(json!({"op":"defer","to":"2026-09-30"})),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "closed items need reopen: {body}"
    );
}

#[tokio::test]
async fn finishing_an_item_closes_its_reminders() {
    let h = harness();
    let (_, todo) = authed(
        &h.app,
        Method::POST,
        "/v1/items",
        Some(json!({"title":"发小红书","day":"2026-09-28"})),
    )
    .await;
    let id = todo["id"].as_str().unwrap().to_owned();
    for tags in [json!(["planner", format!("item:{id}")]), json!(["planner"])] {
        let (status, _) = authed(
            &h.app,
            Method::POST,
            "/v1/events",
            Some(json!({"source":"magpie:planner","title":"20:00 发小红书","severity":"urgent","tags":tags})),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);
    }

    let (status, _) = authed(
        &h.app,
        Method::PATCH,
        &format!("/v1/items/{id}"),
        Some(json!({"op":"done"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (_, open) = authed(&h.app, Method::GET, "/v1/events?open=true", None).await;
    let open = open.as_array().unwrap();
    assert_eq!(
        open.len(),
        1,
        "only the event not tied to the item stays open"
    );
    assert_eq!(open[0]["tags"], json!(["planner"]));
}

#[tokio::test]
async fn item_filters_are_validated() {
    let h = harness();
    for bad in [
        "/v1/items?day=2026-09-29&before=2026-09-29",
        "/v1/items?from=2026-09-30&to=2026-09-29",
        "/v1/items?from=2026-09-29",
        "/v1/items?day=tomorrow",
        "/v1/items?stauts=open",
    ] {
        let (status, _) = authed(&h.app, Method::GET, bad, None).await;
        assert!(status.is_client_error(), "{bad} -> {status}");
    }
    let (status, _) = authed(
        &h.app,
        Method::GET,
        &format!("/v1/items/{}", uuid::Uuid::new_v4()),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = authed(
        &h.app,
        Method::POST,
        "/v1/items",
        Some(json!({"title":"x","dayy":"2026-09-29"})),
    )
    .await;
    assert!(status.is_client_error(), "unknown field rejected");
}

#[tokio::test]
async fn plan_draft_then_apply() {
    let h = harness();
    let add = |body: Value| {
        let app = h.app.clone();
        async move { authed(&app, Method::POST, "/v1/items", Some(body)).await.1 }
    };
    let overdue = add(json!({"title":"昨天没做","day":"2026-09-27","estimate_min":30})).await;
    let fixed =
        add(json!({"title":"20:00 发帖","day":"2026-09-28","at":"20:00:00","estimate_min":10}))
            .await;
    let huge = add(json!({"title":"大改版","day":"2026-09-28","estimate_min":170})).await;
    add(json!({"kind":"idea","title":"点子不会被自动排"})).await;

    let (status, draft) = authed(&h.app, Method::GET, "/v1/plan?day=2026-09-28", None).await;
    assert_eq!(status, StatusCode::OK, "{draft}");
    assert_eq!(draft["capacity_min"], 180);
    let planned: Vec<&str> = draft["planned"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["title"].as_str().unwrap())
        .collect();
    assert_eq!(planned, ["20:00 发帖", "昨天没做"]);
    assert_eq!(draft["pushed"][0]["title"], "大改版");
    assert_eq!(draft["pushed"][0]["to"], "2026-09-29");

    // The draft changed nothing.
    let (_, still) = authed(
        &h.app,
        Method::GET,
        &format!("/v1/items/{}", huge["id"].as_str().unwrap()),
        None,
    )
    .await;
    assert_eq!(still["day"], "2026-09-28");

    let (status, applied) = authed(
        &h.app,
        Method::POST,
        "/v1/plan/apply",
        Some(json!({"day":"2026-09-28"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{applied}");
    let get = |id: &Value| {
        let app = h.app.clone();
        let id = id.as_str().unwrap().to_owned();
        async move {
            authed(&app, Method::GET, &format!("/v1/items/{id}"), None)
                .await
                .1
        }
    };
    assert_eq!(
        get(&overdue["id"]).await["day"],
        "2026-09-28",
        "overdue pulled into the day"
    );
    let moved = get(&huge["id"]).await;
    assert_eq!(
        (moved["day"].as_str(), moved["deferrals"].as_u64()),
        (Some("2026-09-29"), Some(1))
    );
    assert_eq!(get(&fixed["id"]).await["day"], "2026-09-28");

    let (status, _) = authed(&h.app, Method::GET, "/v1/plan", None).await;
    assert!(status.is_client_error(), "day is required");
}
