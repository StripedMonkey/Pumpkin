use std::{
    collections::HashMap,
    io::Cursor,
    sync::{
        atomic::{AtomicI32, Ordering},
        Arc,
    },
    time::Duration,
};

use base64::{engine::general_purpose, Engine};
use image::GenericImageView;
use num_traits::ToPrimitive;
use pumpkin_entity::{entity_type::EntityType, EntityId};
use pumpkin_protocol::{
    bytebuf::ByteBuffer,
    client::{
        config::CPluginMessage,
        play::{
            CCenterChunk, CChunkData, CGameEvent, CLogin, CPlayerAbilities, CPlayerInfoUpdate,
            CRemoveEntities, CRemovePlayerInfo, CSetEntityMetadata, CSpawnEntity, Metadata,
            PlayerAction,
        },
    },
    uuid::UUID,
    ClientPacket, Players, Sample, StatusResponse, VarInt, Version, CURRENT_MC_PROTOCOL,
};
use pumpkin_world::{dimension::Dimension, radial_chunk_iterator::RadialIterator, World};

use pumpkin_registry::Registry;
use rsa::{traits::PublicKeyParts, RsaPrivateKey, RsaPublicKey};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, Mutex, MutexGuard};

use crate::{
    client::Client,
    config::{AdvancedConfiguration, BasicConfiguration},
    entity::player::{GameMode, Player},
};

pub const CURRENT_MC_VERSION: &str = "1.21.1";

pub struct Server {
    pub compression_threshold: Option<u8>,

    pub public_key: RsaPublicKey,
    pub private_key: RsaPrivateKey,
    pub public_key_der: Box<[u8]>,

    pub world: Arc<Mutex<World>>,
    pub status_response: StatusResponse,
    // We cache the json response here so we don't parse it every time someone makes a Status request.
    // Keep in mind that we must parse this again, when the StatusResponse changes which usally happen when a player joins or leaves
    pub status_response_json: String,

    /// Cache the Server brand buffer so we don't have to rebuild them every time a player joins
    pub cached_server_brand: Vec<u8>,

    /// Cache the registry so we don't have to parse it every time a player joins
    pub cached_registry: Vec<Registry>,

    pub current_clients: HashMap<u32, Arc<Mutex<Client>>>,

    // TODO: replace with HashMap <World, Player>
    entity_id: AtomicI32, // TODO: place this into every world
    pub base_config: BasicConfiguration,
    pub advanced_config: AdvancedConfiguration,

    /// Used for Authentication, None is Online mode is disabled
    pub auth_client: Option<reqwest::Client>,
}

impl Server {
    pub fn new(config: (BasicConfiguration, AdvancedConfiguration)) -> Self {
        let status_response = Self::build_response(&config.0);
        let status_response_json = serde_json::to_string(&status_response)
            .expect("Failed to parse Status response into JSON");
        let cached_server_brand = Self::build_brand();

        // TODO: only create when needed
        dbg!("creating keys");
        let (public_key, private_key) = Self::generate_keys();

        let public_key_der = rsa_der::public_key_to_der(
            &private_key.n().to_bytes_be(),
            &private_key.e().to_bytes_be(),
        )
        .into_boxed_slice();
        let auth_client = if config.0.online_mode {
            Some(
                reqwest::Client::builder()
                    .timeout(Duration::from_millis(5000))
                    .build()
                    .expect("Failed to to make reqwest client"),
            )
        } else {
            None
        };

        log::debug!("Pumpkin does currently not have World or Chunk generation, Using ../world folder with vanilla pregenerated chunks");
        let world = World::load(Dimension::OverWorld.into_level(
            // TODO: load form config
            "./world".parse().unwrap(),
        ));

        Self {
            cached_registry: Registry::get_static(),
            // 0 is invalid
            entity_id: 2.into(),
            world: Arc::new(Mutex::new(world)),
            compression_threshold: None, // 256
            public_key,
            cached_server_brand,
            private_key,
            status_response,
            status_response_json,
            public_key_der,
            current_clients: HashMap::new(),
            base_config: config.0,
            auth_client,
            advanced_config: config.1,
        }
    }

    pub fn add_client(&mut self, token: u32, client: Arc<Mutex<Client>>) {
        self.current_clients.insert(token, client);
    }

    pub async fn remove_client(&mut self, token: &u32) {
        let client = self.current_clients.remove(token).unwrap();
        let client = client.lock().await;
        // despawn the player
        // todo: put this into the entitiy struct
        if client.is_player() {
            let id = client.player.as_ref().unwrap().entity_id();
            let uuid = client.gameprofile.as_ref().unwrap().id;
            self.broadcast_packet_expect(
                &[client.token],
                &CRemovePlayerInfo::new(1.into(), &[UUID(uuid)]),
            )
            .await;
            self.broadcast_packet_expect(&[client.token], &CRemoveEntities::new(&[id.into()]))
                .await
        }
    }

    // here is where the magic happens
    // TODO: do this in a world
    pub async fn spawn_player(&mut self, client: &mut Client) {
        // This code follows the vanilla packet order
        let entity_id = self.new_entity_id();
        let gamemode = match self.base_config.default_gamemode {
            GameMode::Undefined => GameMode::Survival,
            game_mode => game_mode,
        };
        log::debug!("spawning player, entity id {}", entity_id);
        let player = Player::new(entity_id, gamemode);
        client.player = Some(player);

        // login packet for our new player
        client
            .send_packet(&CLogin::new(
                entity_id,
                self.base_config.hardcore,
                &["minecraft:overworld"],
                self.base_config.max_players.into(),
                self.base_config.view_distance.into(), //  TODO: view distance
                self.base_config.simulation_distance.into(), // TODO: sim view dinstance
                false,
                false,
                false,
                0.into(),
                "minecraft:overworld",
                0, // seed
                gamemode.to_u8().unwrap(),
                self.base_config.default_gamemode.to_i8().unwrap(),
                false,
                false,
                None,
                0.into(),
                false,
            ))
            .await;
        dbg!("sending abilities");
        // player abilities
        client
            .send_packet(&CPlayerAbilities::new(0x02, 0.1, 0.1))
            .await;

        // teleport
        let x = 10.0;
        let y = 120.0;
        let z = 10.0;
        let yaw = 10.0;
        let pitch = 10.0;
        client.teleport(x, y, z, 10.0, 10.0).await;
        let gameprofile = client.gameprofile.as_ref().unwrap();
        // first send info update to our new player, So he can see his Skin
        // also send his info to everyone else
        self.broadcast_packet(
            client,
            &CPlayerInfoUpdate::new(
                0x01 | 0x08,
                &[pumpkin_protocol::client::play::Player {
                    uuid: gameprofile.id,
                    actions: vec![
                        PlayerAction::AddPlayer {
                            name: gameprofile.name.clone(),
                            properties: gameprofile.properties.clone(),
                        },
                        PlayerAction::UpdateListed { listed: true },
                    ],
                }],
            ),
        )
        .await;

        // here we send all the infos of already joined players
        let mut entries = Vec::new();
        for (_, client) in self.current_clients.iter().filter(|c| c.0 != &client.token) {
            let client = client.blocking_lock();
            if client.is_player() {
                let gameprofile = client.gameprofile.as_ref().unwrap();
                entries.push(pumpkin_protocol::client::play::Player {
                    uuid: gameprofile.id,
                    actions: vec![
                        PlayerAction::AddPlayer {
                            name: gameprofile.name.clone(),
                            properties: gameprofile.properties.clone(),
                        },
                        PlayerAction::UpdateListed { listed: true },
                    ],
                })
            }
        }
        client
            .send_packet(&CPlayerInfoUpdate::new(0x01 | 0x08, &entries))
            .await;

        // Start waiting for level chunks
        client.send_packet(&CGameEvent::new(13, 0.0)).await;

        let gameprofile = client.gameprofile.as_ref().unwrap();

        // spawn player for every client
        self.broadcast_packet_expect(
            &[client.token],
            // TODO: add velo
            &CSpawnEntity::new(
                entity_id.into(),
                UUID(gameprofile.id),
                EntityType::Player.to_i32().unwrap().into(),
                x,
                y,
                z,
                pitch,
                yaw,
                yaw,
                0.into(),
                0.0,
                0.0,
                0.0,
            ),
        )
        .await;
        // spawn players for our client
        let token = client.token;
        for (_, existing_client) in self.current_clients.iter().filter(|c| c.0 != &token) {
            let existing_client = existing_client.blocking_lock();
            if let Some(player) = &existing_client.player {
                let entity = &player.entity;
                let gameprofile = existing_client.gameprofile.as_ref().unwrap();
                client
                    .send_packet(&CSpawnEntity::new(
                        player.entity_id().into(),
                        UUID(gameprofile.id),
                        EntityType::Player.to_i32().unwrap().into(),
                        entity.x,
                        entity.y,
                        entity.z,
                        entity.yaw,
                        entity.pitch,
                        entity.pitch,
                        0.into(),
                        0.0,
                        0.0,
                        0.0,
                    ))
                    .await;
            }
        }
        // entity meta data
        if let Some(config) = &client.config {
            self.broadcast_packet(
                client,
                &CSetEntityMetadata::new(
                    entity_id.into(),
                    Metadata::new(17, VarInt(0), config.skin_parts),
                ),
            )
            .await
        }

        self.spawn_test_chunk(client, self.base_config.view_distance as u32)
            .await;
    }

    /// TODO: This definitly should be in world
    pub fn get_by_entityid(&self, from: &Client, id: EntityId) -> Option<MutexGuard<Client>> {
        for (_, client) in self.current_clients.iter().filter(|c| c.0 != &from.token) {
            // Check if client is a player
            let client = client.blocking_lock();
            if client.is_player() && client.player.as_ref().unwrap().entity_id() == id {
                return Some(client);
            }
        }
        None
    }

    /// Sends a Packet to all Players
    pub async fn broadcast_packet<P>(&self, from: &mut Client, packet: &P)
    where
        P: ClientPacket,
    {
        // we can't borrow twice at same time
        from.send_packet(packet).await;
        for (_, client) in self.current_clients.iter().filter(|c| c.0 != &from.token) {
            // Check if client is a player
            let mut client = client.blocking_lock();
            if client.is_player() {
                client.send_packet(packet).await;
            }
        }
    }

    pub async fn broadcast_packet_expect<P>(&self, from: &[u32], packet: &P)
    where
        P: ClientPacket,
    {
        for (_, client) in self.current_clients.iter().filter(|c| !from.contains(c.0)) {
            // Check if client is a player
            let mut client = client.blocking_lock();
            if client.is_player() {
                client.send_packet(packet).await;
            }
        }
    }

    // TODO: do this in a world
    async fn spawn_test_chunk(&self, client: &mut Client, distance: u32) {
        let inst = std::time::Instant::now();
        let (sender, mut chunk_receiver) = mpsc::channel(distance as usize);
        let world = self.world.clone();
        tokio::spawn(async move {
            world
                .lock()
                .await
                .level
                .read_chunks(RadialIterator::new(distance).collect(), sender)
                .await;
        });

        client
            .send_packet(&CCenterChunk {
                chunk_x: 0.into(),
                chunk_z: 0.into(),
            })
            .await;

        while let Some((chunk_pos, chunk_data)) = chunk_receiver.recv().await {
            // dbg!(chunk_pos);
            let chunk_data = match chunk_data {
                Ok(d) => d,
                Err(_) => continue,
            };
            #[cfg(debug_assertions)]
            if chunk_pos == (0, 0) {
                let mut test = ByteBuffer::empty();
                CChunkData(&chunk_data).write(&mut test);
                let len = test.buf().len();
                log::debug!(
                    "Chunk packet size: {}B {}KB {}MB",
                    len,
                    len / 1024,
                    len / (1024 * 1024)
                );
            }
            client.send_packet(&CChunkData(&chunk_data)).await;
        }
        let t = inst.elapsed();
        dbg!("DONE", t);
    }

    // move to world
    pub fn new_entity_id(&self) -> EntityId {
        self.entity_id.fetch_add(1, Ordering::SeqCst)
    }

    pub fn build_brand() -> Vec<u8> {
        let brand = "Pumpkin";
        let mut buf = vec![];
        let _ = VarInt(brand.len() as i32).encode(&mut buf);
        buf.extend_from_slice(brand.as_bytes());
        buf
    }

    pub async fn send_brand(&self, client: &mut Client) {
        // send server brand
        client
            .send_packet(&CPluginMessage::new(
                "minecraft:brand",
                &self.cached_server_brand,
            ))
            .await;
    }

    pub fn build_response(config: &BasicConfiguration) -> StatusResponse {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/icon.png");

        StatusResponse {
            version: Version {
                name: CURRENT_MC_VERSION.into(),
                protocol: CURRENT_MC_PROTOCOL,
            },
            players: Players {
                max: config.max_players,
                online: 0,
                sample: vec![Sample {
                    name: "".into(),
                    id: "".into(),
                }],
            },
            description: config.motd.clone(),
            favicon: Self::load_icon(path),
        }
    }

    pub fn load_icon(path: &str) -> String {
        let icon = match image::open(path).map_err(|e| panic!("error loading icon: {}", e)) {
            Ok(icon) => icon,
            Err(_) => return "".into(),
        };
        let dimension = icon.dimensions();
        assert!(dimension.0 == 64, "Icon width must be 64");
        assert!(dimension.1 == 64, "Icon height must be 64");
        let mut image = Vec::with_capacity(64 * 64 * 4);
        icon.write_to(&mut Cursor::new(&mut image), image::ImageFormat::Png)
            .unwrap();
        let mut result = "data:image/png;base64,".to_owned();
        general_purpose::STANDARD.encode_string(image, &mut result);
        result
    }

    pub fn generate_keys() -> (RsaPublicKey, RsaPrivateKey) {
        let mut rng = rand::thread_rng();

        let priv_key = RsaPrivateKey::new(&mut rng, 1024).expect("failed to generate a key");
        let pub_key = RsaPublicKey::from(&priv_key);
        (pub_key, priv_key)
    }
}

#[derive(PartialEq, Serialize, Deserialize)]
pub enum Difficulty {
    Peaceful,
    Easy,
    Normal,
    Hard,
}
