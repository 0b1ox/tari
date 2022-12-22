//   Copyright 2022. The Tari Project
//
//   Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//   following conditions are met:
//
//   1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//   disclaimer.
//
//   2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//   following disclaimer in the documentation and/or other materials provided with the distribution.
//
//   3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//   products derived from this software without specific prior written permission.
//
//   THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//   INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//   DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//   SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//   SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//   WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//   USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

mod utils;

use std::{path::PathBuf, time::Duration};

use cucumber::{gherkin::Scenario, given, then, when, World as _};
use indexmap::IndexMap;
use tari_app_grpc::tari_rpc::{TransactionKernel, TransactionOutput};
use tari_base_node_grpc_client::grpc::{Empty, GetBalanceRequest};
use tari_common::initialize_logging;
use tari_crypto::tari_utilities::ByteArray;
use tari_integration_tests::error::GrpcBaseNodeError;
use tari_wallet_grpc_client::grpc::GetIdentityRequest;
use thiserror::Error;
use utils::miner::{mine_block_with_coinbase_on_node, mine_blocks_without_wallet};

use crate::utils::{
    base_node_process::{spawn_base_node, BaseNodeProcess},
    miner::{register_miner_process, MinerProcess},
    wallet_process::{spawn_wallet, WalletProcess},
};

#[derive(Error, Debug)]
pub enum TariWorldError {
    #[error("Base node process not found: {0}")]
    BaseNodeProcessNotFound(String),
    #[error("Wallet process not found: {0}")]
    WalletProcessNotFound(String),
    #[error("Miner process not found: {0}")]
    MinerProcessNotFound(String),
    #[error("Base node error: {0}")]
    GrpcBaseNodeError(#[from] GrpcBaseNodeError),
    #[error("No base node, or wallet client found: {0}")]
    ClientNotFound(String),
}

#[derive(Debug, Default, cucumber::World)]
pub struct TariWorld {
    seed_nodes: Vec<String>,
    base_nodes: IndexMap<String, BaseNodeProcess>,
    wallets: IndexMap<String, WalletProcess>,
    miners: IndexMap<String, MinerProcess>,
    coinbases: IndexMap<String, (TransactionOutput, TransactionKernel)>,
}

enum NodeClient {
    BaseNode(tari_base_node_grpc_client::BaseNodeGrpcClient<tonic::transport::Channel>),
    Wallet(tari_wallet_grpc_client::WalletGrpcClient<tonic::transport::Channel>),
}

impl TariWorld {
    async fn get_node_client<S: AsRef<str>>(
        &self,
        name: &S,
    ) -> anyhow::Result<tari_base_node_grpc_client::BaseNodeGrpcClient<tonic::transport::Channel>> {
        self.get_node(name)?.get_grpc_client().await
    }

    async fn get_base_node_or_wallet_client<S: core::fmt::Debug + AsRef<str>>(
        &self,
        name: S,
    ) -> anyhow::Result<NodeClient> {
        match self.get_node_client(&name).await {
            Ok(client) => Ok(NodeClient::BaseNode(client)),
            Err(_) => match self.get_wallet_client(&name).await {
                Ok(wallet) => Ok(NodeClient::Wallet(wallet)),
                Err(e) => Err(TariWorldError::ClientNotFound(e.to_string()).into()),
            },
        }
    }

    async fn get_wallet_client<S: AsRef<str>>(
        &self,
        name: &S,
    ) -> anyhow::Result<tari_wallet_grpc_client::WalletGrpcClient<tonic::transport::Channel>> {
        self.get_wallet(name)?.get_grpc_client().await
    }

    fn get_node<S: AsRef<str>>(&self, node_name: &S) -> anyhow::Result<&BaseNodeProcess> {
        Ok(self
            .base_nodes
            .get(node_name.as_ref())
            .ok_or_else(|| TariWorldError::BaseNodeProcessNotFound(node_name.as_ref().to_string()))?)
    }

    fn get_wallet<S: AsRef<str>>(&self, wallet_name: &S) -> anyhow::Result<&WalletProcess> {
        Ok(self
            .wallets
            .get(wallet_name.as_ref())
            .ok_or_else(|| TariWorldError::WalletProcessNotFound(wallet_name.as_ref().to_string()))?)
    }

    fn get_miner<S: AsRef<str>>(&self, miner_name: S) -> anyhow::Result<&MinerProcess> {
        Ok(self
            .miners
            .get(miner_name.as_ref())
            .ok_or_else(|| TariWorldError::MinerProcessNotFound(miner_name.as_ref().to_string()))?)
    }

    pub fn all_seed_nodes(&self) -> &[String] {
        self.seed_nodes.as_slice()
    }

    pub async fn after(&mut self, _scenario: &Scenario) {
        self.base_nodes.clear();
        self.seed_nodes.clear();
        self.wallets.clear();
        self.miners.clear();
    }
}

#[given(expr = "I have a seed node {word}")]
#[when(expr = "I have a seed node {word}")]
async fn start_base_node(world: &mut TariWorld, name: String) {
    spawn_base_node(world, true, name, vec![]).await;
}

#[given(expr = "I have wallet {word} connected to base node {word}")]
#[when(expr = "I have wallet {word} connected to base node {word}")]
async fn start_wallet(world: &mut TariWorld, wallet_name: String, node_name: String) {
    let seeds = world.base_nodes.get(&node_name).unwrap().seed_nodes.clone();
    spawn_wallet(world, wallet_name, Some(node_name), seeds).await;
}

#[when(expr = "I have a base node {word} connected to all seed nodes")]
async fn start_base_node_connected_to_all_seed_nodes(world: &mut TariWorld, name: String) {
    spawn_base_node(world, false, name, world.all_seed_nodes().to_vec()).await;
}

#[when(expr = "I have wallet {word} connected to all seed nodes")]
async fn start_wallet_connected_to_all_seed_nodes(world: &mut TariWorld, name: String) {
    spawn_wallet(world, name, None, world.all_seed_nodes().to_vec()).await;
}

#[when(expr = "I have mining node {word} connected to base node {word} and wallet {word}")]
async fn create_miner(world: &mut TariWorld, miner_name: String, bn_name: String, wallet_name: String) {
    register_miner_process(world, miner_name, bn_name, wallet_name);
}

#[when(expr = "I wait {int} seconds")]
async fn wait_seconds(_world: &mut TariWorld, seconds: u64) {
    tokio::time::sleep(Duration::from_secs(seconds)).await;
}

#[when(expr = "I wait for {word} to connect to {word}")]
#[then(expr = "I wait for {word} to connect to {word}")]
#[then(expr = "{word} is connected to {word}")]
async fn node_pending_connection_to(
    world: &mut TariWorld,
    first_node: String,
    second_node: String,
) -> anyhow::Result<()> {
    let mut first_node = world.get_base_node_or_wallet_client(&first_node).await?;
    let mut second_node = world.get_base_node_or_wallet_client(&second_node).await?;
    for _i in 0..100 {
        let res = match first_node {
            NodeClient::BaseNode(ref mut client) => client.list_connected_peers(Empty {}).await?,
            NodeClient::Wallet(ref mut client) => client.list_connected_peers(Empty {}).await?,
        };
        let res = res.into_inner();

        let public_key = match second_node {
            NodeClient::BaseNode(ref mut client) => client.identify(Empty {}).await?.into_inner().public_key,
            NodeClient::Wallet(ref mut client) => client.identify(GetIdentityRequest {}).await?.into_inner().public_key,
        };

        if res
            .connected_peers
            .iter()
            .any(|p| p.public_key == public_key.as_bytes())
        {
            return Ok(());
        }

        tokio::time::sleep(Duration::from_secs(5)).await;
    }

    panic!("Peer was not connected in time");
}

#[when(expr = "mining node {word} mines {int} blocks")]
#[given(expr = "mining node {word} mines {int} blocks")]
async fn run_miner(world: &mut TariWorld, miner_name: String, num_blocks: u64) {
    world.get_miner(miner_name).unwrap().mine(world, Some(num_blocks)).await;
}

#[then(expr = "all nodes are at height {int}")]
#[when(expr = "all nodes are at height {int}")]
async fn all_nodes_are_at_height(world: &mut TariWorld, height: u64) -> anyhow::Result<()> {
    let num_retries = 100;
    let mut already_sync = true;

    for _ in 0..num_retries {
        for (_, bn) in world.base_nodes.iter() {
            let mut client = bn.get_grpc_client().await?;

            let chain_tip = client.get_tip_info(Empty {}).await?.into_inner();
            let chain_hgt = chain_tip.metadata.unwrap().height_of_longest_chain;

            if chain_hgt < height {
                already_sync = false;
            }
        }

        if already_sync {
            return Ok(());
        }

        already_sync = true;
        tokio::time::sleep(Duration::from_secs(5)).await;
    }

    if !already_sync {
        panic!("base nodes not successfully synchronized at height {}", height);
    }

    Ok(())
}

#[when(expr = "node {word} is at height {int}")]
#[then(expr = "node {word} is at height {int}")]
async fn node_is_at_height(world: &mut TariWorld, base_node: String, height: u64) {
    let num_retries = 100;

    let mut client = world
        .base_nodes
        .get(&base_node)
        .unwrap()
        .get_grpc_client()
        .await
        .unwrap();
    let mut chain_hgt = 0;

    for _ in 0..=num_retries {
        let chain_tip = client.get_tip_info(Empty {}).await.unwrap().into_inner();
        chain_hgt = chain_tip.metadata.unwrap().height_of_longest_chain;

        if chain_hgt >= height {
            return;
        }

        tokio::time::sleep(Duration::from_secs(5)).await;
    }

    // base node didn't synchronize successfully at height, so we bail out
    panic!(
        "base node didn't synchronize successfully with height {}, current chain height {}",
        height, chain_hgt
    );
}

#[when(expr = "I wait for wallet {word} to have at least {int} uT")]
async fn wait_for_wallet_to_have_micro_tari(world: &mut TariWorld, wallet: String, amount: u64) {
    let wallet = world.wallets.get(&wallet).unwrap();
    let num_retries = 100;

    let mut client = wallet.get_grpc_client().await.unwrap();
    let mut curr_amount = 0;

    for _ in 0..=num_retries {
        curr_amount = client
            .get_balance(GetBalanceRequest {})
            .await
            .unwrap()
            .into_inner()
            .available_balance;

        if curr_amount >= amount {
            return;
        }

        tokio::time::sleep(Duration::from_secs(5)).await;
    }

    // failed to get wallet right amount, so we panic
    panic!(
        "wallet failed to get right amount {}, current amount is {}",
        amount, curr_amount
    );
}

#[given(expr = "I have a base node {word} connected to seed {word}")]
#[when(expr = "I have a base node {word} connected to seed {word}")]
async fn base_node_connected_to_seed(world: &mut TariWorld, base_node: String, seed: String) {
    spawn_base_node(world, false, base_node, vec![seed]).await;
}

#[then(expr = "I mine {int} blocks on {word}")]
#[when(expr = "I mine {int} blocks on {word}")]
async fn mine_blocks_on(world: &mut TariWorld, base_node: String, blocks: u64) {
    let mut client = world
        .base_nodes
        .get(&base_node)
        .unwrap()
        .get_grpc_client()
        .await
        .unwrap();
    mine_blocks_without_wallet(&mut client, blocks).await;
}

#[when(expr = "mining node {word} mines {int} blocks with min difficulty {int} and max difficulty {int}")]
async fn mining_node_mines_blocks_with_difficulty(
    _world: &mut TariWorld,
    _miner: String,
    _blocks: u64,
    _min_difficulty: u64,
    _max_difficulty: u64,
) {
}

#[when(expr = "I have a base node {word}")]
#[given(expr = "I have a base node {word}")]
async fn create_and_add_base_node(world: &mut TariWorld, base_node: String) {
    spawn_base_node(world, false, base_node, vec![]).await;
}

#[given(expr = "I have {int} seed nodes")]
async fn have_seed_nodes(world: &mut TariWorld, seed_nodes: u64) {
    for node in 0..seed_nodes {
        spawn_base_node(world, true, format!("seed_node_{}", node), vec![]).await;
    }
}

#[when(expr = "I have wallet {word} connected to seed node {word}")]
async fn have_wallet_connect_to_seed_node(world: &mut TariWorld, wallet: String, seed_node: String) {
    spawn_wallet(world, wallet, None, vec![seed_node]).await;
}

#[when(expr = "I mine a block on {word} with coinbase {word}")]
async fn mine_block_with_coinbase_on_node_step(world: &mut TariWorld, base_node: String, coinbase_name: String) {
    mine_block_with_coinbase_on_node(world, base_node, coinbase_name).await;
}

#[when(expr = "I print the cucumber world")]
async fn print_world(world: &mut TariWorld) {
    eprintln!();
    eprintln!("======================================");
    eprintln!("============= TEST NODES =============");
    eprintln!("======================================");
    eprintln!();

    // base nodes
    for (name, node) in world.base_nodes.iter() {
        eprintln!(
            "Base node \"{}\": grpc port \"{}\", temp dir path \"{}\"",
            name, node.grpc_port, node.temp_dir_path
        );
    }

    // wallets
    for (name, node) in world.wallets.iter() {
        eprintln!(
            "Wallet \"{}\": grpc port \"{}\", temp dir path \"{}\"",
            name, node.grpc_port, node.temp_dir_path
        );
    }

    eprintln!();
    eprintln!("======================================");
    eprintln!();
}

#[tokio::main]
async fn main() {
    initialize_logging(
        &PathBuf::from("log4rs/base_node.yml"),
        include_str!("../log4rs/base_node.yml"),
    )
    .expect("logging not configured");
    TariWorld::cucumber()
        .repeat_failed()
        // following config needed to use eprint statements in the tests
        //.max_concurrent_scenarios(1)
        //.with_writer(
        //    writer::Basic::raw(io::stdout(), writer::Coloring::Never, 0)
        //        .summarized()
        //        .assert_normalized(),
        //)
        .after(|_feature,_rule,scenario,_ev,maybe_world| {
            Box::pin(async move {
                if let Some(maybe_world) = maybe_world {
                    maybe_world.after(scenario).await;
                }
            })
        })
        .run_and_exit("tests/features/")
        .await;
}
