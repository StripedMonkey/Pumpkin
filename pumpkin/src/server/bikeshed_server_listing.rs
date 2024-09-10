use std::{io::Cursor, path::Path};

use base64::{engine::general_purpose, Engine as _};
use image::GenericImageView as _;
use pumpkin_config::{BasicConfiguration, BASIC_CONFIG};
use pumpkin_protocol::{
    client::{config::CPluginMessage, status::CStatusResponse},
    Players, Sample, StatusResponse, VarInt, Version, CURRENT_MC_PROTOCOL,
};

use super::CURRENT_MC_VERSION;

pub struct BikeShedServerListing {
    _status_response: StatusResponse,
    // We cache the json response here so we don't parse it every time someone makes a Status request.
    // Keep in mind that we must parse this again, when the StatusResponse changes which usually happen when a player joins or leaves
    status_response_json: String,
    /// Cached Server brand buffer so we don't have to rebuild them every time a player joins
    cached_server_brand: Vec<u8>,
}

impl BikeShedServerListing {
    pub fn new() -> Self {
        let status_response = Self::build_response(&BASIC_CONFIG);
        let status_response_json = serde_json::to_string(&status_response)
            .expect("Failed to parse Status response into JSON");
        let cached_server_brand = Self::build_brand();

        BikeShedServerListing {
            _status_response: status_response,
            status_response_json,
            cached_server_brand,
        }
    }

    pub fn get_branding(&self) -> CPluginMessage {
        CPluginMessage::new("minecraft:brand", &self.cached_server_brand)
    }

    pub fn get_status(&self) -> CStatusResponse<'_> {
        CStatusResponse::new(&self.status_response_json)
    }

    pub fn build_response(config: &BasicConfiguration) -> StatusResponse {
        let icon_path = concat!(env!("CARGO_MANIFEST_DIR"), "/icon.png");
        let icon = if Path::new(icon_path).exists() {
            Some(Self::load_icon(icon_path))
        } else {
            None
        };

        StatusResponse {
            version: Some(Version {
                name: CURRENT_MC_VERSION.into(),
                protocol: CURRENT_MC_PROTOCOL,
            }),
            players: Some(Players {
                max: config.max_players,
                online: 0,
                sample: vec![Sample {
                    name: "".into(),
                    id: "".into(),
                }],
            }),
            description: config.motd.clone(),
            favicon: icon,
            enforece_secure_chat: false,
        }
    }

    fn load_icon(path: &str) -> String {
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

    fn build_brand() -> Vec<u8> {
        let brand = "Pumpkin";
        let mut buf = vec![];
        let _ = VarInt(brand.len() as i32).encode(&mut buf);
        buf.extend_from_slice(brand.as_bytes());
        buf
    }
}
