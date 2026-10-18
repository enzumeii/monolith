mod inventories;
mod orders;

use crate::common::{authed, client_with_upstream_script, fixture};
use axum::{body::Body, http::Request};
use domain::dto::AdminRole;

#[tokio::test]
async fn reports_are_not_exposed_outside_the_admin_namespace() {
    let fixture = fixture().await;
    let (client, upstream) = client_with_upstream_script(vec![], "http").await;
    for uri in [
        "/v1/inventories/1",
        "/v1/orders/report?tags=report",
        "/v1/customer/inventories/1",
        "/v1/customer/orders/report?tags=report",
    ] {
        let mut requests = vec![Request::builder().uri(uri).body(Body::empty()).unwrap()];
        for role in [AdminRole::Admin, AdminRole::SuperAdmin] {
            requests.push(authed("GET", uri, fixture.root_id(), role));
        }
        for request in requests {
            let response = client.send(request).await.unwrap();
            assert_eq!(response.status(), 404, "{uri}");
            assert!(response.headers().get("www-authenticate").is_none());
        }
    }
    assert!(upstream.requests().is_empty());
}
