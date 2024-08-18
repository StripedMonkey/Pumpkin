use std::future::Future;

use gamemode::GamemodeCommand;
use pumpkin::PumpkinCommand;
use pumpkin_text::TextComponent;
use stop::StopCommand;

use crate::client::Client;

mod gamemode;
mod pumpkin;
mod stop;

/// I think it would be great to split this up into a seperate crate, But idk how i should do that, Because we have to rely on Client and Server
pub trait Command<'a> {
    // Name of the Plugin, Use lower case
    const NAME: &'a str;
    const DESCRIPTION: &'a str;

    fn on_execute(sender: &mut CommandSender<'a>, command: String) -> impl Future<Output=()>;

    /// Specifies wether the Command Sender has to be a Player
    /// TODO: implement
    fn player_required() -> bool {
        false
    }
}

pub enum CommandSender<'a> {
    Rcon(&'a mut Vec<String>),
    Console,
    Player(&'a mut Client),
}

impl<'a> CommandSender<'a> {
    pub async fn send_message<'b>(&mut self, text: TextComponent<'b>) {
        match self {
            // TODO: add color and stuff to console
            CommandSender::Console => log::info!("{:?}", text.content),
            CommandSender::Player(c) => c.send_system_message(text).await,
            CommandSender::Rcon(s) => s.push(format!("{:?}", text.content)),
        }
    }

    pub fn is_player(&mut self) -> bool {
        match self {
            CommandSender::Console => false,
            CommandSender::Player(_) => true,
            CommandSender::Rcon(_) => false,
        }
    }

    pub fn is_console(&mut self) -> bool {
        match self {
            CommandSender::Console => true,
            CommandSender::Player(_) => false,
            CommandSender::Rcon(_) => true,
        }
    }
    pub fn as_mut_player(&mut self) -> Option<&mut Client> {
        match self {
            CommandSender::Player(client) => Some(client),
            CommandSender::Console => None,
            CommandSender::Rcon(_) => None,
        }
    }
}
pub async fn handle_command<'a>(sender: &mut CommandSender<'a>, command: &str) {
    let command = command.to_lowercase();
    // an ugly mess i know
    if command.starts_with(PumpkinCommand::NAME) {
        PumpkinCommand::on_execute(sender, command).await;
        return;
    }
    if command.starts_with(GamemodeCommand::NAME) {
        GamemodeCommand::on_execute(sender, command).await;
        return;
    }
    if command.starts_with(StopCommand::NAME) {
        StopCommand::on_execute(sender, command).await;
        return;
    }
    // TODO: red color
    sender
        .send_message(TextComponent::text("Command not Found"))
        .await;
}
