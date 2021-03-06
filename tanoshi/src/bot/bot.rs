use std::sync::mpsc::{channel, Sender};
use tbot::contexts::fields::Message;
use tbot::{prelude::*, types::chat::Id, types::parameters::Text, Bot as TBot};

pub enum TextType {
    #[allow(dead_code)]
    Plain,
    #[allow(dead_code)]
    Markdown,
    #[allow(dead_code)]
    MarkdownV2,
    #[allow(dead_code)]
    HTML,
}

#[derive(Clone)]
pub struct Bot {
    bot: TBot,
    tx: Sender<(Id, String, TextType)>,
}

impl Bot {
    pub fn new(token: String) -> Self {
        let bot = TBot::new(token);
        let (tx, rx) = channel::<(Id, String, TextType)>();

        let bot_clone = bot.clone();
        tokio::spawn(async move {
            loop {
                let res = rx.recv();
                match res {
                    Ok((id, data, text_type)) => {
                        let text = match text_type {
                            TextType::Plain => Text::plain(data.as_str()),
                            TextType::Markdown => Text::markdown(data.as_str()),
                            TextType::MarkdownV2 => Text::markdown_v2(data.as_str()),
                            TextType::HTML => Text::html(data.as_str()),
                        };
                        bot_clone.send_message(id, text).call().await.unwrap();
                    }
                    Err(e) => {
                        error!("error receive data: {}", e);
                        break;
                    }
                }
            }
        });

        Bot { bot, tx }
    }

    pub fn start(&self) {
        let bot = self.bot.clone();
        tokio::spawn(async move {
            let mut bot = bot.event_loop();

            bot.start(|context| async move {
                    let chat_id = context.clone().chat().id.0;
                    context.send_message(
                        Text::plain(
                            format!("This chat id is {}. Input this chat id in setting to get notification on chapter updates and relogin", chat_id)
                                .as_str()
                        ))
                        .call()
                        .await
                        .unwrap();
                });

            if let Err(_) = bot.polling().start().await {
                error!("error start telegram bot polling")
            }
        });
    }

    pub fn get_pub(&self) -> Sender<(Id, String, TextType)> {
        self.tx.clone()
    }
}
