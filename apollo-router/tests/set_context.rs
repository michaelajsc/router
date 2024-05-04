//!
//! Please ensure that any tests added to this file use the tokio multi-threaded test executor.
//!

use apollo_router::graphql::Request;
use apollo_router::graphql::Response;
use apollo_router::plugin::test::MockSubgraph;
use apollo_router::services::supergraph;
use apollo_router::MockedSubgraphs;
use apollo_router::TestHarness;
use serde::Deserialize;
use serde_json::json;
use tower::ServiceExt;

#[derive(Deserialize)]
struct SubgraphMock {
    mocks: Vec<RequestAndResponse>,
}

#[derive(Deserialize)]
struct RequestAndResponse {
    request: Request,
    response: Response,
}

#[tokio::test(flavor = "multi_thread")]
async fn test_set_context() {
    let harness = setup_from_mocks(
        json! {{
            "experimental_type_conditioned_fetching": true,
            // will make debugging easier
            "plugins": {
                "experimental.expose_query_plan": true
            },
            "include_subgraph_errors": {
                "all": true
            }
        }},
        &[
            ("Subgraph1", include_str!("fixtures/set_context/one.json")),
            ("Subgraph2", include_str!("fixtures/set_context/two.json")),
        ],
    );
    let supergraph_service = harness.build_supergraph().await.unwrap();
    let request = supergraph::Request::fake_builder()
        .query(QUERY.to_string())
        .header("Apollo-Expose-Query-Plan", "true")
        .variables(Default::default())
        .build()
        .expect("expecting valid request");

    let response = supergraph_service
        .oneshot(request)
        .await
        .unwrap()
        .next_response()
        .await
        .unwrap();

    insta::assert_json_snapshot!(response);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_set_context_no_typenames() {
    let harness = setup_from_mocks(
        json! {{
            "experimental_type_conditioned_fetching": true,
            // will make debugging easier
            "plugins": {
                "experimental.expose_query_plan": true
            },
            "include_subgraph_errors": {
                "all": true
            }
        }},
        &[
            ("Subgraph1", include_str!("fixtures/set_context/one.json")),
            ("Subgraph2", include_str!("fixtures/set_context/two.json")),
        ],
    );
    let supergraph_service = harness.build_supergraph().await.unwrap();
    let request = supergraph::Request::fake_builder()
        .query(QUERY_NO_TYPENAMES.to_string())
        .header("Apollo-Expose-Query-Plan", "true")
        .variables(Default::default())
        .build()
        .expect("expecting valid request");

    let response = supergraph_service
        .oneshot(request)
        .await
        .unwrap()
        .next_response()
        .await
        .unwrap();

    insta::assert_json_snapshot!(response);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_set_context_list() {
    let harness = setup_from_mocks(
        json! {{
            "experimental_type_conditioned_fetching": true,
            // will make debugging easier
            "plugins": {
                "experimental.expose_query_plan": true
            },
            "include_subgraph_errors": {
                "all": true
            }
        }},
        &[
            ("Subgraph1", include_str!("fixtures/set_context/one.json")),
            ("Subgraph2", include_str!("fixtures/set_context/two.json")),
        ],
    );
    let supergraph_service = harness.build_supergraph().await.unwrap();
    let request = supergraph::Request::fake_builder()
        .query(QUERY_WITH_LIST.to_string())
        .header("Apollo-Expose-Query-Plan", "true")
        .variables(Default::default())
        .build()
        .expect("expecting valid request");

    let response = supergraph_service
        .oneshot(request)
        .await
        .unwrap()
        .next_response()
        .await
        .unwrap();

    insta::assert_json_snapshot!(response);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_set_context_list_of_lists() {
    let harness = setup_from_mocks(
        json! {{
            "experimental_type_conditioned_fetching": true,
            // will make debugging easier
            "plugins": {
                "experimental.expose_query_plan": true
            },
            "include_subgraph_errors": {
                "all": true
            }
        }},
        &[
            ("Subgraph1", include_str!("fixtures/set_context/one.json")),
            ("Subgraph2", include_str!("fixtures/set_context/two.json")),
        ],
    );
    let supergraph_service = harness.build_supergraph().await.unwrap();
    let request: supergraph::Request = supergraph::Request::fake_builder()
        .query(QUERY_WITH_LIST_OF_LISTS.to_string())
        .header("Apollo-Expose-Query-Plan", "true")
        .variables(Default::default())
        .build()
        .expect("expecting valid request");

    let response = supergraph_service
        .oneshot(request)
        .await
        .unwrap()
        .next_response()
        .await
        .unwrap();

    insta::assert_json_snapshot!(response);
}

fn setup_from_mocks(
    configuration: serde_json::Value,
    mocks: &[(&'static str, &'static str)],
) -> TestHarness<'static> {
    let mut mocked_subgraphs = MockedSubgraphs::default();

    for (name, m) in mocks {
        let subgraph_mock: SubgraphMock = serde_json::from_str(m).unwrap();

        let mut builder = MockSubgraph::builder();

        for mock in subgraph_mock.mocks {
            builder = builder.with_json(
                serde_json::to_value(mock.request).unwrap(),
                serde_json::to_value(mock.response).unwrap(),
            );
        }

        mocked_subgraphs.insert(name, builder.build());
    }

    let schema = include_str!("fixtures/set_context/supergraph.graphql");
    TestHarness::builder()
        .try_log_level("info")
        .configuration_json(configuration)
        .unwrap()
        .schema(schema)
        .extra_plugin(mocked_subgraphs)
}

// TODO[clenfest]: figure out why i need __typename here?
static QUERY: &str = r#"query Query {
        t {
          __typename
          id
          u {
            __typename
            field
          }
        }
      }"#;
static QUERY_NO_TYPENAMES: &str = r#"query Query {
        t {
          id
          u {
            field
          }
        }
      }"#;

static QUERY_WITH_LIST: &str = r#"query Query {
        t {
          id
          uList {
            field
          }
        }
      }"#;

static QUERY_WITH_LIST_OF_LISTS: &str = r#"query QueryLL {
        tList {
          id
          uList {
            field
          }
        }
      }"#;
