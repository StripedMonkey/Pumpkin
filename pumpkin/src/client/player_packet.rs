use std::{f32::consts::PI, sync::Arc};

use crate::{
    commands::CommandSender,
    entity::player::{ChatMode, Hand, Player},
    server::Server,
    world::player_chunker,
};
use num_traits::FromPrimitive;
use pumpkin_config::ADVANCED_CONFIG;
use pumpkin_core::{
    math::{position::WorldPosition, vector3::Vector3, wrap_degrees},
    text::TextComponent,
    GameMode,
};
use pumpkin_entity::EntityId;
use pumpkin_inventory::{InventoryError, WindowType};
use pumpkin_protocol::server::play::{SCloseContainer, SSetPlayerGround, SUseItem};
use pumpkin_protocol::{
    client::play::{
        Animation, CAcknowledgeBlockChange, CBlockUpdate, CEntityAnimation, CEntityVelocity,
        CHeadRot, CHurtAnimation, CPingResponse, CPlayerChatMessage, CUpdateEntityPos,
        CUpdateEntityPosRot, CUpdateEntityRot, CWorldEvent, FilterType,
    },
    server::play::{
        Action, ActionType, SChatCommand, SChatMessage, SClientInformationPlay, SConfirmTeleport,
        SInteract, SPlayPingRequest, SPlayerAction, SPlayerCommand, SPlayerPosition,
        SPlayerPositionRotation, SPlayerRotation, SSetCreativeSlot, SSetHeldItem, SSwingArm,
        SUseItemOn, Status,
    },
};
use pumpkin_world::block::{BlockFace, BlockId};
use pumpkin_world::global_registry;

use super::PlayerConfig;

fn modulus(a: f32, b: f32) -> f32 {
    ((a % b) + b) % b
}

/// Handles all Play Packets send by a real Player
/// NEVER TRUST THE CLIENT. HANDLE EVERY ERROR, UNWRAP/EXPECT ARE FORBIDDEN
impl Player {
    pub fn handle_confirm_teleport(
        &self,
        _server: &Arc<Server>,
        confirm_teleport: SConfirmTeleport,
    ) {
        let mut awaiting_teleport = self.awaiting_teleport.lock();
        if let Some((id, position)) = awaiting_teleport.as_ref() {
            if id == &confirm_teleport.teleport_id {
                // we should set the pos now to that we requested in the teleport packet, Is may fixed issues when the client sended position packets while being teleported
                self.entity.set_pos(position.x, position.y, position.z);

                *awaiting_teleport = None;
            } else {
                self.kick(TextComponent::text("Wrong teleport id"))
            }
        } else {
            self.kick(TextComponent::text(
                "Send Teleport confirm, but we did not teleport",
            ))
        }
    }

    fn clamp_horizontal(pos: f64) -> f64 {
        pos.clamp(-3.0E7, 3.0E7)
    }

    fn clamp_vertical(pos: f64) -> f64 {
        pos.clamp(-2.0E7, 2.0E7)
    }

    pub async fn handle_position(&self, _server: &Arc<Server>, position: SPlayerPosition) {
        if position.x.is_nan() || position.feet_y.is_nan() || position.z.is_nan() {
            self.kick(TextComponent::text("Invalid movement"));
            return;
        }
        let entity = &self.entity;
        entity.set_pos(
            Self::clamp_horizontal(position.x),
            Self::clamp_vertical(position.feet_y),
            Self::clamp_horizontal(position.z),
        );
        let pos = entity.pos.load();
        self.last_position.store(pos);
        let last_position = self.last_position.load();
        entity
            .on_ground
            .store(position.ground, std::sync::atomic::Ordering::Relaxed);
        let entity_id = entity.entity_id;
        let Vector3 { x, y, z } = pos;
        let (lastx, lasty, lastz) = (last_position.x, last_position.y, last_position.z);
        let world = &entity.world;

        // let delta = Vector3::new(x - lastx, y - lasty, z - lastz);
        // let velocity = self.velocity;

        // // Player is falling down fast, we should account for that
        // let max_speed = if self.fall_flying { 300.0 } else { 100.0 };

        // teleport when more than 8 blocks (i guess 8 blocks)
        // TODO: REPLACE * 2.0 by movement packets. see vanilla for details
        // if delta.length_squared() - velocity.length_squared() > max_speed * 2.0 {
        //     self.teleport(x, y, z, self.entity.yaw, self.entity.pitch);
        //     return;
        // }
        // send new position to all other players
        world.broadcast_packet_expect(
            &[self.client.token],
            &CUpdateEntityPos::new(
                entity_id.into(),
                (x * 4096.0 - lastx * 4096.0) as i16,
                (y * 4096.0 - lasty * 4096.0) as i16,
                (z * 4096.0 - lastz * 4096.0) as i16,
                position.ground,
            ),
        );
        player_chunker::update_position(entity, self).await;
    }

    pub async fn handle_position_rotation(
        &self,
        _server: &Arc<Server>,
        position_rotation: SPlayerPositionRotation,
    ) {
        if position_rotation.x.is_nan()
            || position_rotation.feet_y.is_nan()
            || position_rotation.z.is_nan()
        {
            self.kick(TextComponent::text("Invalid movement"));
            return;
        }
        if !position_rotation.yaw.is_finite() || !position_rotation.pitch.is_finite() {
            self.kick(TextComponent::text("Invalid rotation"));
            return;
        }
        let entity = &self.entity;

        entity.set_pos(
            Self::clamp_horizontal(position_rotation.x),
            Self::clamp_vertical(position_rotation.feet_y),
            Self::clamp_horizontal(position_rotation.z),
        );
        let pos = entity.pos.load();
        self.last_position.store(pos);
        let last_position = self.last_position.load();
        entity.on_ground.store(
            position_rotation.ground,
            std::sync::atomic::Ordering::Relaxed,
        );
        entity.set_rotation(
            wrap_degrees(position_rotation.yaw) % 360.0,
            wrap_degrees(position_rotation.pitch).clamp(-90.0, 90.0) % 360.0,
        );

        let entity_id = entity.entity_id;
        let Vector3 { x, y, z } = pos;
        let (lastx, lasty, lastz) = (last_position.x, last_position.y, last_position.z);
        let yaw = modulus(entity.yaw.load() * 256.0 / 360.0, 256.0);
        let pitch = modulus(entity.pitch.load() * 256.0 / 360.0, 256.0);
        // let head_yaw = (entity.head_yaw * 256.0 / 360.0).floor();
        let world = &entity.world;

        // let delta = Vector3::new(x - lastx, y - lasty, z - lastz);
        // let velocity = self.velocity;

        // // Player is falling down fast, we should account for that
        // let max_speed = if self.fall_flying { 300.0 } else { 100.0 };

        // // teleport when more than 8 blocks (i guess 8 blocks)
        // // TODO: REPLACE * 2.0 by movement packets. see vanilla for details
        // if delta.length_squared() - velocity.length_squared() > max_speed * 2.0 {
        //     self.teleport(x, y, z, yaw, pitch);
        //     return;
        // }
        // send new position to all other players

        world.broadcast_packet_expect(
            &[self.client.token],
            &CUpdateEntityPosRot::new(
                entity_id.into(),
                (x * 4096.0 - lastx * 4096.0) as i16,
                (y * 4096.0 - lasty * 4096.0) as i16,
                (z * 4096.0 - lastz * 4096.0) as i16,
                yaw as u8,
                pitch as u8,
                position_rotation.ground,
            ),
        );
        world.broadcast_packet_expect(
            &[self.client.token],
            &CHeadRot::new(entity_id.into(), yaw as u8),
        );
        player_chunker::update_position(entity, self).await;
    }

    pub async fn handle_rotation(&self, _server: &Arc<Server>, rotation: SPlayerRotation) {
        if !rotation.yaw.is_finite() || !rotation.pitch.is_finite() {
            self.kick(TextComponent::text("Invalid rotation"));
            return;
        }
        let entity = &self.entity;
        entity
            .on_ground
            .store(rotation.ground, std::sync::atomic::Ordering::Relaxed);
        entity.set_rotation(
            wrap_degrees(rotation.yaw) % 360.0,
            wrap_degrees(rotation.pitch).clamp(-90.0, 90.0) % 360.0,
        );
        // send new position to all other players
        let entity_id = entity.entity_id;
        let yaw = modulus(entity.yaw.load() * 256.0 / 360.0, 256.0);
        let pitch = modulus(entity.pitch.load() * 256.0 / 360.0, 256.0);
        // let head_yaw = modulus(entity.head_yaw * 256.0 / 360.0, 256.0);

        let world = &entity.world;
        let packet =
            CUpdateEntityRot::new(entity_id.into(), yaw as u8, pitch as u8, rotation.ground);
        world.broadcast_packet_expect(&[self.client.token], &packet);
        let packet = CHeadRot::new(entity_id.into(), yaw as u8);
        world.broadcast_packet_expect(&[self.client.token], &packet);
    }

    pub fn handle_chat_command(&self, server: &Arc<Server>, command: SChatCommand) {
        let dispatcher = server.command_dispatcher.clone();
        dispatcher.handle_command(&mut CommandSender::Player(self), server, &command.command);
    }

    pub fn handle_player_ground(&self, _server: &Arc<Server>, ground: SSetPlayerGround) {
        self.entity
            .on_ground
            .store(ground.on_ground, std::sync::atomic::Ordering::Relaxed);
    }

    pub async fn handle_player_command(&self, _server: &Arc<Server>, command: SPlayerCommand) {
        if command.entity_id != self.entity_id().into() {
            return;
        }

        if let Some(action) = Action::from_i32(command.action.0) {
            let entity = &self.entity;
            match action {
                pumpkin_protocol::server::play::Action::StartSneaking => {
                    if !entity.sneaking.load(std::sync::atomic::Ordering::Relaxed) {
                        entity.set_sneaking(true).await
                    }
                }
                pumpkin_protocol::server::play::Action::StopSneaking => {
                    if entity.sneaking.load(std::sync::atomic::Ordering::Relaxed) {
                        entity.set_sneaking(false).await
                    }
                }
                pumpkin_protocol::server::play::Action::LeaveBed => todo!(),
                pumpkin_protocol::server::play::Action::StartSprinting => {
                    if !entity.sprinting.load(std::sync::atomic::Ordering::Relaxed) {
                        entity.set_sprinting(true).await
                    }
                }
                pumpkin_protocol::server::play::Action::StopSprinting => {
                    if entity.sprinting.load(std::sync::atomic::Ordering::Relaxed) {
                        entity.set_sprinting(false).await
                    }
                }
                pumpkin_protocol::server::play::Action::StartHorseJump => todo!(),
                pumpkin_protocol::server::play::Action::StopHorseJump => todo!(),
                pumpkin_protocol::server::play::Action::OpenVehicleInventory => todo!(),
                pumpkin_protocol::server::play::Action::StartFlyingElytra => {
                    let fall_flying = entity.check_fall_flying();
                    if entity
                        .fall_flying
                        .load(std::sync::atomic::Ordering::Relaxed)
                        != fall_flying
                    {
                        entity.set_fall_flying(fall_flying).await;
                    }
                } // TODO
            }
        } else {
            self.kick(TextComponent::text("Invalid player command"))
        }
    }

    pub async fn handle_swing_arm(&self, _server: &Arc<Server>, swing_arm: SSwingArm) {
        match Hand::from_i32(swing_arm.hand.0) {
            Some(hand) => {
                let animation = match hand {
                    Hand::Main => Animation::SwingMainArm,
                    Hand::Off => Animation::SwingOffhand,
                };
                let id = self.entity_id();
                let world = &self.entity.world;
                world.broadcast_packet_expect(
                    &[self.client.token],
                    &CEntityAnimation::new(id.into(), animation as u8),
                )
            }
            None => {
                self.kick(TextComponent::text("Invalid hand"));
            }
        };
    }

    pub async fn handle_chat_message(&self, _server: &Arc<Server>, chat_message: SChatMessage) {
        dbg!("got message");

        let message = chat_message.message;
        if message.len() > 256 {
            self.kick(TextComponent::text("Oversized message"));
            return;
        }

        // TODO: filter message & validation
        let gameprofile = &self.gameprofile;

        let entity = &self.entity;
        let world = &entity.world;
        world.broadcast_packet_all(&CPlayerChatMessage::new(
            gameprofile.id,
            1.into(),
            chat_message.signature.as_deref(),
            &message,
            chat_message.timestamp,
            chat_message.salt,
            &[],
            Some(TextComponent::text(&message)),
            FilterType::PassThrough,
            1.into(),
            TextComponent::text(&gameprofile.name),
            None,
        ))

        /* server.broadcast_packet(
            self,
            &CDisguisedChatMessage::new(
                TextComponent::from(message.clone()),
                VarInt(0),
                gameprofile.name.clone().into(),
                None,
            ),
        ) */
    }

    pub fn handle_client_information_play(
        &self,
        _server: &Arc<Server>,
        client_information: SClientInformationPlay,
    ) {
        if let (Some(main_hand), Some(chat_mode)) = (
            Hand::from_i32(client_information.main_hand.into()),
            ChatMode::from_i32(client_information.chat_mode.into()),
        ) {
            *self.config.lock() = PlayerConfig {
                locale: client_information.locale,
                view_distance: client_information.view_distance,
                chat_mode,
                chat_colors: client_information.chat_colors,
                skin_parts: client_information.skin_parts,
                main_hand,
                text_filtering: client_information.text_filtering,
                server_listing: client_information.server_listing,
            };
        } else {
            self.kick(TextComponent::text("Invalid hand or chat type"))
        }
    }

    pub async fn handle_interact(&self, _: &Arc<Server>, interact: SInteract) {
        let sneaking = interact.sneaking;
        let entity = &self.entity;
        if entity.sneaking.load(std::sync::atomic::Ordering::Relaxed) != sneaking {
            entity.set_sneaking(sneaking).await;
        }
        match ActionType::from_i32(interact.typ.0) {
            Some(action) => match action {
                ActionType::Attack => {
                    let entity_id = interact.entity_id;
                    // TODO: do validation and stuff
                    let config = &ADVANCED_CONFIG.pvp;
                    if config.enabled {
                        let world = &entity.world;
                        let attacked_player = world.get_player_by_entityid(entity_id.0 as EntityId);
                        if let Some(player) = attacked_player {
                            let victem_entity = &player.entity;
                            if config.protect_creative
                                && player.gamemode.load() == GameMode::Creative
                            {
                                return;
                            }
                            if config.knockback {
                                let yaw = entity.yaw.load();
                                let strength = 1.0;
                                let victem_velocity = victem_entity.velocity.load();
                                let saved_velo = victem_velocity;
                                victem_entity.knockback(
                                    strength * 0.5,
                                    (yaw * (PI / 180.0)).sin() as f64,
                                    -(yaw * (PI / 180.0)).cos() as f64,
                                );
                                let packet = &CEntityVelocity::new(
                                    &entity_id,
                                    victem_velocity.x as f32,
                                    victem_velocity.y as f32,
                                    victem_velocity.z as f32,
                                );
                                let velocity = entity.velocity.load();
                                victem_entity
                                    .velocity
                                    .store(velocity.multiply(0.6, 1.0, 0.6));

                                victem_entity.velocity.store(saved_velo);
                                player.client.send_packet(packet);
                            }
                            if config.hurt_animation {
                                world.broadcast_packet_all(&CHurtAnimation::new(
                                    &entity_id,
                                    entity.yaw.load(),
                                ))
                            }
                            if config.swing {}
                        } else {
                            self.kick(TextComponent::text("Interacted with invalid entity id"))
                        }
                    }
                }
                ActionType::Interact => {
                    dbg!("todo");
                }
                ActionType::InteractAt => {
                    dbg!("todo");
                }
            },
            None => self.kick(TextComponent::text("Invalid action type")),
        }
    }
    pub async fn handle_player_action(&self, _server: &Arc<Server>, player_action: SPlayerAction) {
        match Status::from_i32(player_action.status.0) {
            Some(status) => match status {
                Status::StartedDigging => {
                    if !self.can_interact_with_block_at(&player_action.location, 1.0) {
                        // TODO: maybe log?
                        return;
                    }
                    // TODO: do validation
                    // TODO: Config
                    if self.gamemode.load() == GameMode::Creative {
                        let location = player_action.location;
                        // Block break & block break sound
                        // TODO: currently this is always dirt replace it
                        let entity = &self.entity;
                        let world = &entity.world;
                        world.broadcast_packet_all(&CWorldEvent::new(2001, &location, 11, false));
                        // AIR
                        world.broadcast_packet_all(&CBlockUpdate::new(&location, 0.into()));
                    }
                }
                Status::CancelledDigging => {
                    if !self.can_interact_with_block_at(&player_action.location, 1.0) {
                        // TODO: maybe log?
                        return;
                    }
                    self.current_block_destroy_stage
                        .store(0, std::sync::atomic::Ordering::Relaxed);
                }
                Status::FinishedDigging => {
                    // TODO: do validation
                    let location = player_action.location;
                    if !self.can_interact_with_block_at(&location, 1.0) {
                        // TODO: maybe log?
                        return;
                    }
                    // Block break & block break sound
                    // TODO: currently this is always dirt replace it
                    let entity = &self.entity;
                    let world = &entity.world;
                    world.broadcast_packet_all(&CWorldEvent::new(2001, &location, 11, false));
                    // AIR
                    world.broadcast_packet_all(&CBlockUpdate::new(&location, 0.into()));
                    // TODO: Send this every tick
                    self.client
                        .send_packet(&CAcknowledgeBlockChange::new(player_action.sequence));
                }
                Status::DropItemStack => {
                    dbg!("todo");
                }
                Status::DropItem => {
                    dbg!("todo");
                }
                Status::ShootArrowOrFinishEating => {
                    dbg!("todo");
                }
                Status::SwapItem => {
                    dbg!("todo");
                }
            },
            None => self.kick(TextComponent::text("Invalid status")),
        }
    }

    pub fn handle_play_ping_request(&self, _server: &Arc<Server>, request: SPlayPingRequest) {
        self.client
            .send_packet(&CPingResponse::new(request.payload));
    }

    pub async fn handle_use_item_on(&self, _server: &Arc<Server>, use_item_on: SUseItemOn) {
        let location = use_item_on.location;

        if !self.can_interact_with_block_at(&location, 1.0) {
            // TODO: maybe log?
            return;
        }

        if let Some(face) = BlockFace::from_i32(use_item_on.face.0) {
            if let Some(item) = self.inventory.lock().held_item() {
                let minecraft_id = global_registry::find_minecraft_id(
                    global_registry::ITEM_REGISTRY,
                    item.item_id,
                )
                .expect("All item ids are in the global registry");
                if let Ok(block_state_id) = BlockId::new(minecraft_id, None) {
                    let entity = &self.entity;
                    let world = &entity.world;
                    world.broadcast_packet_all(&CBlockUpdate::new(
                        &location,
                        block_state_id.get_id_mojang_repr().into(),
                    ));
                    world.broadcast_packet_all(&CBlockUpdate::new(
                        &WorldPosition(location.0 + face.to_offset()),
                        block_state_id.get_id_mojang_repr().into(),
                    ));
                }
            }
            self.client
                .send_packet(&CAcknowledgeBlockChange::new(use_item_on.sequence));
        } else {
            self.kick(TextComponent::text("Invalid block face"))
        }
    }

    pub fn handle_use_item(&self, _server: &Arc<Server>, _use_item: SUseItem) {
        // TODO: handle packet correctly
        log::error!("An item was used(SUseItem), but the packet is not implemented yet");
    }

    pub fn handle_set_held_item(&self, _server: &Arc<Server>, held: SSetHeldItem) {
        let slot = held.slot;
        if !(0..=8).contains(&slot) {
            self.kick(TextComponent::text("Invalid held slot"))
        }
        self.inventory.lock().set_selected(slot as usize);
    }

    pub fn handle_set_creative_slot(
        &self,
        _server: &Arc<Server>,
        packet: SSetCreativeSlot,
    ) -> Result<(), InventoryError> {
        if self.gamemode.load() != GameMode::Creative {
            return Err(InventoryError::PermissionError);
        }
        self.inventory
            .lock()
            .set_slot(packet.slot as usize, packet.clicked_item.to_item(), false)
    }

    // TODO:
    // This function will in the future be used to keep track of if the client is in a valid state.
    // But this is not possible yet
    pub fn handle_close_container(&self, server: &Arc<Server>, packet: SCloseContainer) {
        // window_id 0 represents both 9x1 Generic AND inventory here
        self.inventory
            .lock()
            .state_id
            .store(0, std::sync::atomic::Ordering::Relaxed);
        let open_container = self.open_container.load();
        if let Some(id) = open_container {
            let mut open_containers = server.open_containers.write();
            if let Some(container) = open_containers.get_mut(&id) {
                container.remove_player(self.entity_id())
            }
            self.open_container.store(None);
        }
        let Some(_window_type) = WindowType::from_u8(packet.window_id) else {
            self.kick(TextComponent::text("Invalid window ID"));
            return;
        };
    }
}
