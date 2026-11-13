//! `/items` end to end through the production router and middleware stack.

mod common;

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use common::{TEST_KEY, TestResponse, app, ready_state, send, state_with};
use gitops_api::modules::items::application::ports::{
    BoxFuture, ItemPage, ItemRepository, Page, RepositoryError,
};
use gitops_api::modules::items::domain::{Item, ItemId};
use serde_json::{Value, json};

// ---------------------------------------------------------------- helpers

fn router() -> Router {
    app(ready_state("test"))
}

fn request(method: Method, uri: &str, has_body: bool) -> axum::http::request::Builder {
    let builder = Request::builder().method(method).uri(uri);
    if has_body {
        builder.header("content-type", "application/json")
    } else {
        builder
    }
}

async fn call(router: &Router, method: Method, uri: &str, body: Option<Value>) -> TestResponse {
    let req = request(method, uri, body.is_some())
        .header("authorization", format!("Bearer {TEST_KEY}"))
        .body(body.map_or_else(Body::empty, |b| Body::from(b.to_string())))
        .unwrap();
    send(router.clone(), req).await
}

async fn raw(router: &Router, req: Request<Body>) -> TestResponse {
    send(router.clone(), req).await
}

async fn create(router: &Router, name: &str) -> Value {
    let res = call(
        router,
        Method::POST,
        "/items",
        Some(json!({ "name": name })),
    )
    .await;
    assert_eq!(res.status, StatusCode::CREATED, "{:?}", res.json());
    res.json()
}

fn assert_error(res: &TestResponse, status: StatusCode, code: &str) {
    assert_eq!(
        res.status,
        status,
        "body: {}",
        String::from_utf8_lossy(&res.body)
    );
    assert_eq!(res.header("content-type"), "application/json");
    assert_eq!(res.json()["status"], code);
}

// ------------------------------------------------------------- auth

#[tokio::test]
async fn every_items_route_requires_a_valid_key() {
    let router = router();
    let item_id = ItemId::generate().to_string();
    let endpoints = [
        (Method::GET, "/items".to_owned()),
        (Method::POST, "/items".to_owned()),
        (Method::GET, format!("/items/{item_id}")),
        (Method::PUT, format!("/items/{item_id}")),
        (Method::DELETE, format!("/items/{item_id}")),
    ];
    let bad_credentials = [
        None,
        Some("Bearer wrong-key-0123456789abcdef0123456789abcdef".to_owned()),
        Some(format!("Basic {TEST_KEY}")),
        Some(TEST_KEY.to_owned()),
        Some("Bearer ".to_owned()),
    ];

    for (method, uri) in &endpoints {
        for credential in &bad_credentials {
            let mut req = Request::builder().method(method.clone()).uri(uri);
            if let Some(value) = credential {
                req = req.header("authorization", value);
            }
            let res = raw(&router, req.body(Body::empty()).unwrap()).await;
            assert_error(&res, StatusCode::UNAUTHORIZED, "unauthorized");
            assert_eq!(
                res.header("www-authenticate"),
                r#"Bearer realm="gitops-api""#
            );
            assert!(res.headers.contains_key("x-request-id"));
        }
    }
}

#[tokio::test]
async fn unauthenticated_body_is_never_parsed() {
    // Garbage JSON without a key must be 401, not 400: auth runs first.
    let req = Request::post("/items")
        .header("content-type", "application/json")
        .body(Body::from("{not json"))
        .unwrap();
    assert_error(
        &raw(&router(), req).await,
        StatusCode::UNAUTHORIZED,
        "unauthorized",
    );
}

#[tokio::test]
async fn bearer_scheme_is_case_insensitive() {
    let req = Request::get("/items")
        .header("authorization", format!("bearer {TEST_KEY}"))
        .body(Body::empty())
        .unwrap();
    assert_eq!(raw(&router(), req).await.status, StatusCode::OK);
}

#[tokio::test]
async fn no_configured_keys_means_nothing_gets_in() {
    use gitops_api::modules::items::application::ItemService;
    use gitops_api::modules::items::infrastructure::InMemoryItemRepository;
    use gitops_api::platform::security::api_key::ApiKeys;

    let state = gitops_api::AppState::new(
        "test",
        ApiKeys::none(),
        ItemService::new(Arc::new(InMemoryItemRepository::new(1))),
    );
    let res = call(&app(state), Method::GET, "/items", None).await;
    assert_error(&res, StatusCode::UNAUTHORIZED, "unauthorized");
}

#[tokio::test]
async fn probes_stay_public() {
    for path in ["/health", "/ready", "/version"] {
        let res = raw(&router(), Request::get(path).body(Body::empty()).unwrap()).await;
        assert_eq!(res.status, StatusCode::OK, "{path}");
    }
}

// ------------------------------------------------------------- CRUD

#[tokio::test]
async fn full_lifecycle() {
    let router = router();

    // Create
    let res = call(
        &router,
        Method::POST,
        "/items",
        Some(json!({ "name": "  Widget ", "description": "blue\nand round" })),
    )
    .await;
    assert_eq!(res.status, StatusCode::CREATED);
    let created = res.json();
    let id = created["id"].as_str().unwrap().to_owned();
    assert_eq!(res.header("location"), format!("/items/{id}"));
    assert_eq!(
        created,
        json!({ "id": id, "name": "Widget", "description": "blue\nand round" })
    );

    // Read
    let res = call(&router, Method::GET, &format!("/items/{id}"), None).await;
    assert_eq!(res.status, StatusCode::OK);
    assert_eq!(res.json(), created);

    // List
    let res = call(&router, Method::GET, "/items", None).await;
    assert_eq!(
        res.json(),
        json!({ "items": [created], "total": 1, "limit": 20, "offset": 0 })
    );

    // Replace (description omitted → empty)
    let res = call(
        &router,
        Method::PUT,
        &format!("/items/{id}"),
        Some(json!({ "name": "Gadget" })),
    )
    .await;
    assert_eq!(res.status, StatusCode::OK);
    assert_eq!(
        res.json(),
        json!({ "id": id, "name": "Gadget", "description": "" })
    );

    // Delete
    let res = call(&router, Method::DELETE, &format!("/items/{id}"), None).await;
    assert_eq!(res.status, StatusCode::NO_CONTENT);
    assert_eq!(res.body, Vec::<u8>::new());

    // Gone
    let res = call(&router, Method::GET, &format!("/items/{id}"), None).await;
    assert_error(&res, StatusCode::NOT_FOUND, "not_found");
    let res = call(&router, Method::DELETE, &format!("/items/{id}"), None).await;
    assert_error(&res, StatusCode::NOT_FOUND, "not_found");
}

#[tokio::test]
async fn pagination() {
    let router = router();
    for i in 0..5 {
        create(&router, &format!("item-{i}")).await;
    }

    let res = call(&router, Method::GET, "/items?limit=2&offset=4", None).await;
    let body = res.json();
    assert_eq!(body["items"].as_array().unwrap().len(), 1);
    assert_eq!(
        (
            body["total"].clone(),
            body["limit"].clone(),
            body["offset"].clone()
        ),
        (json!(5), json!(2), json!(4))
    );

    for bad in ["limit=0", "limit=101"] {
        let res = call(&router, Method::GET, &format!("/items?{bad}"), None).await;
        assert_error(&res, StatusCode::UNPROCESSABLE_ENTITY, "validation_failed");
    }
    for bad in ["limit=abc", "offset=-1", "page=2"] {
        let res = call(&router, Method::GET, &format!("/items?{bad}"), None).await;
        assert_error(&res, StatusCode::BAD_REQUEST, "bad_request");
    }
}

#[tokio::test]
async fn duplicate_names_conflict() {
    let router = router();
    let first = create(&router, "Widget").await;
    create(&router, "Other").await;

    let res = call(
        &router,
        Method::POST,
        "/items",
        Some(json!({ "name": "widget" })),
    )
    .await;
    assert_error(&res, StatusCode::CONFLICT, "conflict");

    let id = first["id"].as_str().unwrap();
    let res = call(
        &router,
        Method::PUT,
        &format!("/items/{id}"),
        Some(json!({ "name": "OTHER" })),
    )
    .await;
    assert_error(&res, StatusCode::CONFLICT, "conflict");
}

// ------------------------------------------------------- input validation

#[tokio::test]
async fn domain_validation_is_422_with_a_safe_message() {
    let router = router();
    let cases = [
        (json!({ "name": "" }), "name: must not be empty"),
        (
            json!({ "name": "x".repeat(101) }),
            "name: must be at most 100 characters",
        ),
        (
            json!({ "name": "a\u{1b}[31m" }),
            "name: must not contain control characters",
        ),
        (
            json!({ "name": "ok", "description": "d".repeat(1001) }),
            "description: must be at most 1000 characters",
        ),
    ];
    for (body, message) in cases {
        let res = call(&router, Method::POST, "/items", Some(body)).await;
        assert_error(&res, StatusCode::UNPROCESSABLE_ENTITY, "validation_failed");
        assert_eq!(res.json()["message"], message);
    }
}

#[tokio::test]
async fn schema_violations_are_422() {
    let router = router();
    for body in [
        json!({}),
        json!({ "name": 42 }),
        json!({ "name": "x", "id": "chosen-by-client" }),
        json!({ "name": "x", "admin": true }),
    ] {
        let res = call(&router, Method::POST, "/items", Some(body.clone())).await;
        assert_error(&res, StatusCode::UNPROCESSABLE_ENTITY, "validation_failed");
        assert!(res.json()["message"].is_string(), "{body}");
    }
}

#[tokio::test]
async fn undecodable_requests() {
    let router = router();
    let auth = format!("Bearer {TEST_KEY}");

    let res = raw(
        &router,
        Request::post("/items")
            .header("authorization", &auth)
            .header("content-type", "application/json")
            .body(Body::from("{\"name\":"))
            .unwrap(),
    )
    .await;
    assert_error(&res, StatusCode::BAD_REQUEST, "bad_request");

    let res = raw(
        &router,
        Request::post("/items")
            .header("authorization", &auth)
            .header("content-type", "text/plain")
            .body(Body::from("{\"name\":\"x\"}"))
            .unwrap(),
    )
    .await;
    assert_error(
        &res,
        StatusCode::UNSUPPORTED_MEDIA_TYPE,
        "unsupported_media_type",
    );

    let res = raw(
        &router,
        Request::post("/items")
            .header("authorization", &auth)
            .header("content-type", "application/json")
            .header("content-length", "70000")
            .body(Body::from(vec![b' '; 70_000]))
            .unwrap(),
    )
    .await;
    assert_eq!(res.status, StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn oversized_body_without_content_length_is_json_413() {
    // No Content-Length: the limit layer cannot reject up front, so the JSON
    // extractor hits the limit while reading and must answer in our format.
    let res = raw(
        &router(),
        Request::post("/items")
            .header("authorization", format!("Bearer {TEST_KEY}"))
            .header("content-type", "application/json")
            .body(Body::from(vec![b' '; 70_000]))
            .unwrap(),
    )
    .await;
    assert_error(&res, StatusCode::PAYLOAD_TOO_LARGE, "payload_too_large");
}

#[tokio::test]
async fn malformed_ids_are_rejected_before_storage() {
    let router = router();
    for id in ["not-a-uuid", "123", "..%2F..%2Fetc"] {
        let res = call(&router, Method::GET, &format!("/items/{id}"), None).await;
        assert_error(&res, StatusCode::UNPROCESSABLE_ENTITY, "validation_failed");
        assert_eq!(res.json()["message"], "id: must be a UUID");
    }
}

#[tokio::test]
async fn unsupported_method_is_405() {
    let res = call(&router(), Method::PATCH, "/items", Some(json!({}))).await;
    assert_error(&res, StatusCode::METHOD_NOT_ALLOWED, "method_not_allowed");
}

// --------------------------------------------------------- failure modes

/// Every operation fails, as a database outage would.
struct BrokenRepository;

fn outage<T: Send + 'static>() -> BoxFuture<'static, Result<T, RepositoryError>> {
    Box::pin(async {
        Err(RepositoryError::Unavailable(
            "connection to postgres://admin:hunter2@10.0.0.5 refused".into(),
        ))
    })
}

impl ItemRepository for BrokenRepository {
    fn insert(&self, _: Item) -> BoxFuture<'_, Result<(), RepositoryError>> {
        outage()
    }
    fn get(&self, _: ItemId) -> BoxFuture<'_, Result<Option<Item>, RepositoryError>> {
        outage()
    }
    fn list(&self, _: Page) -> BoxFuture<'_, Result<ItemPage, RepositoryError>> {
        outage()
    }
    fn update(&self, _: Item) -> BoxFuture<'_, Result<bool, RepositoryError>> {
        outage()
    }
    fn delete(&self, _: ItemId) -> BoxFuture<'_, Result<bool, RepositoryError>> {
        outage()
    }
}

#[tokio::test]
async fn storage_failures_are_500_without_leaking_details() {
    let state = state_with("test", Arc::new(BrokenRepository));
    state.set_ready(true);
    let router = app(state);
    let id = ItemId::generate().to_string();

    for (method, uri, body) in [
        (Method::GET, "/items".to_owned(), None),
        (
            Method::POST,
            "/items".to_owned(),
            Some(json!({ "name": "x" })),
        ),
        (Method::GET, format!("/items/{id}"), None),
        (
            Method::PUT,
            format!("/items/{id}"),
            Some(json!({ "name": "x" })),
        ),
        (Method::DELETE, format!("/items/{id}"), None),
    ] {
        let res = call(&router, method, &uri, body).await;
        assert_error(&res, StatusCode::INTERNAL_SERVER_ERROR, "internal_error");
        assert_eq!(res.json(), json!({ "status": "internal_error" }));
    }
}

#[tokio::test]
async fn full_store_is_507() {
    use gitops_api::modules::items::infrastructure::InMemoryItemRepository;

    let state = state_with("test", Arc::new(InMemoryItemRepository::new(1)));
    let router = app(state);
    create(&router, "only").await;
    let res = call(
        &router,
        Method::POST,
        "/items",
        Some(json!({ "name": "one-more" })),
    )
    .await;
    assert_error(
        &res,
        StatusCode::INSUFFICIENT_STORAGE,
        "insufficient_storage",
    );
}
