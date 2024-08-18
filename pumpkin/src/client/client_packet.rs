use num_traits::FromPrimitive;
use pumpkin_protocol::{
    client::{
        config::{CConfigAddResourcePack, CFinishConfig, CKnownPacks, CRegistryData},
        login::{CEncryptionRequest, CLoginSuccess, CSetCompression},
        status::{CPingResponse, CStatusResponse},
    },
    server::{
        config::{SAcknowledgeFinishConfig, SClientInformationConfig, SKnownPacks, SPluginMessage},
        handshake::SHandShake,
        login::{SEncryptionResponse, SLoginAcknowledged, SLoginPluginResponse, SLoginStart},
        status::{SPingRequest, SStatusRequest},
    },
    ConnectionState, KnownPack, CURRENT_MC_PROTOCOL,
};
use pumpkin_text::TextComponent;
use rsa::Pkcs1v15Encrypt;
use sha1::{Digest, Sha1};

use crate::{
    client::{
        authentication::{self, GameProfile},
        client::PlayerConfig,
    },
    entity::player::{ChatMode, Hand},
    proxy::velocity::velocity_login,
    server::{Server, CURRENT_MC_VERSION},
};

use super::{
    authentication::{auth_digest, unpack_textures},
    client::Client,
    EncryptionError,
};

/// Processes incoming Packets from the Client to the Server
/// Implements the `Client` Packets
impl Client {
    pub async fn handle_handshake(&mut self, _server: &mut Server, handshake: SHandShake) {
        dbg!("handshake");
        self.protocol_version = handshake.protocol_version.0;
        self.connection_state = handshake.next_state;
        if self.connection_state == ConnectionState::Login {
            let protocol = self.protocol_version;
            match protocol.cmp(&(CURRENT_MC_PROTOCOL as i32)) {
                std::cmp::Ordering::Less => {
                    self.kick(&format!("Client outdated ({protocol}), Server uses Minecraft {CURRENT_MC_VERSION}, Protocol {CURRENT_MC_PROTOCOL}")).await;
                }
                std::cmp::Ordering::Equal => {}
                std::cmp::Ordering::Greater => {
                    self.kick(&format!("Server outdated, Server uses Minecraft {CURRENT_MC_VERSION}, Protocol {CURRENT_MC_PROTOCOL}")).await;
                }
            }
        }
    }

    pub async fn handle_status_request(
        &mut self,
        server: &mut Server,
        _status_request: SStatusRequest,
    ) {
        self.send_packet(&CStatusResponse::new(&server.status_response_json))
            .await;
    }

    pub async fn handle_ping_request(&mut self, _server: &mut Server, ping_request: SPingRequest) {
        dbg!("ping");
        self.send_packet(&CPingResponse::new(ping_request.payload))
            .await;
        self.close();
    }

    fn is_valid_player_name(name: &str) -> bool {
        name.len() <= 16 && name.chars().all(|c| c > 32 as char && c < 127 as char)
    }

    pub async fn handle_login_start(&mut self, server: &mut Server, login_start: SLoginStart) {
        dbg!("login start");

        if !Self::is_valid_player_name(&login_start.name) {
            self.kick("Invalid characters in username").await;
            return;
        }
        // default game profile, when no online mode
        // TODO: make offline uuid
        self.gameprofile = Some(GameProfile {
            id: login_start.uuid,
            name: login_start.name,
            properties: vec![],
            profile_actions: None,
        });
        let proxy = &server.advanced_config.proxy;
        if proxy.enabled {
            if proxy.velocity.enabled {
                velocity_login(self).await
            }
            return;
        }

        // TODO: check config for encryption
        let verify_token: [u8; 4] = rand::random();
        let public_key_der = &server.public_key_der;
        let packet = CEncryptionRequest::new(
            "",
            public_key_der,
            &verify_token,
            server.base_config.online_mode, // TODO
        );
        self.send_packet(&packet).await;
    }

    pub async fn handle_encryption_response(
        &mut self,
        server: &mut Server,
        encryption_response: SEncryptionResponse,
    ) {
        let shared_secret = server
            .private_key
            .decrypt(Pkcs1v15Encrypt, &encryption_response.shared_secret)
            .map_err(|_| EncryptionError::FailedDecrypt)
            .unwrap();
        match self.enable_encryption(&shared_secret) {
            Ok(d) => d,
            Err(e) => {
                self.kick(&e.to_string()).await;
            }
        }

        if server.base_config.online_mode {
            let hash = Sha1::new()
                .chain_update(&shared_secret)
                .chain_update(&server.public_key_der)
                .finalize();
            let hash = auth_digest(&hash);
            let ip = self.address.ip();
            match authentication::authenticate(
                &self.gameprofile.as_ref().unwrap().name,
                &hash,
                &ip,
                server,
            )
            .await
            {
                Ok(p) => {
                    // Check if player should join
                    if let Some(p) = &p.profile_actions {
                        if !server
                            .advanced_config
                            .authentication
                            .player_profile
                            .allow_banned_players
                        {
                            if !p.is_empty() {
                                self.kick("Your account can't join").await;
                            }
                        } else {
                            for allowed in server
                                .advanced_config
                                .authentication
                                .player_profile
                                .allowed_actions
                                .clone()
                            {
                                if !p.contains(&allowed) {
                                    self.kick("Your account can't join").await;
                                }
                            }
                        }
                    }
                    self.gameprofile = Some(p);
                }
                Err(e) => self.kick(&e.to_string()).await,
            }
        }
        for ele in self.gameprofile.as_ref().unwrap().properties.clone() {
            // todo, use this
            unpack_textures(ele, &server.advanced_config.authentication.textures);
        }

        // enable compression
        if server.advanced_config.packet_compression.enabled {
            let threshold = server
                .advanced_config
                .packet_compression
                .compression_threshold;
            let level = server.advanced_config.packet_compression.compression_level;
            self.send_packet(&CSetCompression::new(threshold.into()))
                .await;
            self.set_compression(Some((threshold, level)));
        }

        if let Some(profile) = self.gameprofile.as_ref().cloned() {
            let packet = CLoginSuccess::new(profile.id, &profile.name, &profile.properties, false);
            self.send_packet(&packet).await;
        } else {
            self.kick("game profile is none").await;
        }
    }

    pub async fn handle_plugin_response(
        &mut self,
        _server: &mut Server,
        _plugin_response: SLoginPluginResponse,
    ) {
    }

    pub async fn handle_login_acknowledged(
        &mut self,
        server: &mut Server,
        _login_acknowledged: SLoginAcknowledged,
    ) {
        self.connection_state = ConnectionState::Config;
        server.send_brand(self).await;

        let resource_config = &server.advanced_config.resource_pack;
        if resource_config.enabled {
            let prompt_message = if resource_config.prompt_message.is_empty() {
                None
            } else {
                Some(TextComponent::text(&resource_config.prompt_message))
            };
            self.send_packet(&CConfigAddResourcePack::new(
                pumpkin_protocol::uuid::UUID(uuid::Uuid::new_v3(
                    &uuid::Uuid::NAMESPACE_DNS,
                    resource_config.resource_pack_url.as_bytes(),
                )),
                &resource_config.resource_pack_url,
                &resource_config.resource_pack_sha1,
                resource_config.force,
                prompt_message,
            ))
            .await;
        }

        // known data packs
        self.send_packet(&CKnownPacks::new(&[KnownPack {
            namespace: "minecraft",
            id: "core",
            version: "1.21",
        }]))
        .await;
        dbg!("login achnowlaged");
    }
    pub async fn handle_client_information_config(
        &mut self,
        _server: &mut Server,
        client_information: SClientInformationConfig,
    ) {
        dbg!("got client settings");
        self.config = Some(PlayerConfig {
            locale: client_information.locale,
            view_distance: client_information.view_distance,
            chat_mode: ChatMode::from_i32(client_information.chat_mode.into()).unwrap(),
            chat_colors: client_information.chat_colors,
            skin_parts: client_information.skin_parts,
            main_hand: Hand::from_i32(client_information.main_hand.into()).unwrap(),
            text_filtering: client_information.text_filtering,
            server_listing: client_information.server_listing,
        });
    }

    pub async fn handle_plugin_message(
        &mut self,
        _server: &mut Server,
        plugin_message: SPluginMessage,
    ) {
        if plugin_message.channel.starts_with("minecraft:brand")
            || plugin_message.channel.starts_with("MC|Brand")
        {
            dbg!("got a client brand");
            match String::from_utf8(plugin_message.data) {
                Ok(brand) => self.brand = Some(brand),
                Err(e) => self.kick(&e.to_string()).await,
            }
        }
    }

    pub async fn handle_known_packs(
        &mut self,
        server: &mut Server,
        _config_acknowledged: SKnownPacks,
    ) {
        for registry in &server.cached_registry {
            self.send_packet(&CRegistryData::new(
                &registry.registry_id,
                &registry.registry_entries,
            ))
            .await;
        }

        // We are done with configuring
        dbg!("finish config");
        self.send_packet(&CFinishConfig::new()).await;
    }

    pub async fn handle_config_acknowledged(
        &mut self,
        server: &mut Server,
        _config_acknowledged: SAcknowledgeFinishConfig,
    ) {
        dbg!("config acknowledged");
        self.connection_state = ConnectionState::Play;
        // generate a player
        server.spawn_player(self).await;
    }
}
