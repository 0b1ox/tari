//  Copyright 2020, The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use crate::{common::proxy, proxy::MergeMiningProxyConfig};
use hyper::Body;
use tari_common::Network;

fn default_test_config() -> MergeMiningProxyConfig {
    MergeMiningProxyConfig {
        network: Network::Rincewind,
        monerod_url: "".to_string(),
        monerod_username: "".to_string(),
        monerod_password: "".to_string(),
        monerod_use_auth: false,
        grpc_base_node_address: "127.0.0.1:9999".parse().unwrap(),
        grpc_console_wallet_address: "127.0.0.1:9998".parse().unwrap(),
        proxy_host_address: "127.0.0.1:9997".parse().unwrap(),
        proxy_submit_to_origin: false,
        wait_for_initial_sync_at_startup: true,
    }
}

async fn read_body_as_json(body: &mut Body) -> serde_json::Value {
    serde_json::from_slice(&proxy::read_body_until_end(body).await.unwrap()).unwrap()
}

mod merge_mining_proxy_service {
    use super::*;
    use crate::{block_template_data::BlockTemplateRepository, proxy::MergeMiningProxyService};
    use futures::task::Poll;
    use futures_test::task::noop_context;
    use hyper::{service::Service, Body, Request};

    #[test]
    fn it_is_always_ready() {
        let mut service = MergeMiningProxyService::new(default_test_config(), BlockTemplateRepository::new());
        let mut cx = noop_context();
        let poll = service.poll_ready(&mut cx);
        match poll {
            Poll::Ready(v) => v.unwrap(),
            Poll::Pending => panic!("not ready"),
        }
    }

    #[tokio_macros::test]
    async fn it_returns_an_error_response_empty_request() {
        let mut service = MergeMiningProxyService::new(default_test_config(), BlockTemplateRepository::new());
        let req = Request::new(Body::empty());
        let mut resp = service.call(req).await.unwrap();
        assert!(!resp.status().is_success());
        let json = read_body_as_json(resp.body_mut()).await;
        assert_eq!(json["error"]["message"], "Internal error");
    }
}

mod add_aux_data {
    use crate::{
        common::json_rpc,
        proxy::{add_aux_data, MMPROXY_AUX_KEY_NAME},
    };
    use serde_json::json;

    #[test]
    fn it_adds_aux_data() {
        let v = json_rpc::success_response(None, json!({ "hello": "world"}));
        let v = add_aux_data(v, json!({"test": "works"}));
        assert_eq!(v["result"][MMPROXY_AUX_KEY_NAME]["test"].as_str().unwrap(), "works");
    }

    #[test]
    fn it_merges_to_existing_aux_data() {
        let v = json_rpc::success_response(None, json!({ "hello": "world"}));
        let v = add_aux_data(v, json!({"test1": 1}));
        let v = add_aux_data(v, json!({"test2": 2, "test3": 3}));
        assert_eq!(v["result"][MMPROXY_AUX_KEY_NAME]["test1"].as_u64().unwrap(), 1);
        assert_eq!(v["result"][MMPROXY_AUX_KEY_NAME]["test2"].as_u64().unwrap(), 2);
        assert_eq!(v["result"][MMPROXY_AUX_KEY_NAME]["test3"].as_u64().unwrap(), 3);
    }

    #[test]
    fn it_does_not_add_data_to_errors() {
        let v = json_rpc::error_response(None, 1, "it's on 🔥", None);
        let v = add_aux_data(v, json!({"it": "is broken"}));
        assert!(v["result"][MMPROXY_AUX_KEY_NAME]["it"].as_str().is_none());
    }
}

mod append_aux_chain_data {
    use crate::{
        common::json_rpc,
        proxy::{append_aux_chain_data, MMPROXY_AUX_KEY_NAME},
    };
    use serde_json::json;

    #[test]
    fn it_adds_a_chain_object() {
        let v = json_rpc::success_response(None, json!({}));
        let v = append_aux_chain_data(v, json!({"test": "works"}));
        assert_eq!(v["result"][MMPROXY_AUX_KEY_NAME]["chains"].as_array().unwrap(), &[
            json!({"test": "works"})
        ]);
    }
}
