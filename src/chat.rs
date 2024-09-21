use crate::models::converse::call_converse;
use anyhow::anyhow;
use aws_sdk_bedrockruntime::types::{ContentBlock, ConversationRole, Message};
use dialoguer::Confirm;

use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{self, Write},
    fmt::Display,
};
use crate::utils::print_warning;

use colored::*;

use chrono::prelude::*;

use dirs::home_dir;

use crate::constants;

// TODO:
// - Print the summary when recalling the chat- [DONE] ✅
// - Make sure that the filename is correct when saving - enforce chekcks and fallbacks
// - Consider other locations for saving the conversations
// - Produce the print with some syntax highlighting
// - Distinguish between user and computer input in the json
// - Run checks for model support for the hardcoded models

#[derive(Debug, Deserialize, Serialize)]
pub enum ConversationEntity {
    User,
    Assistant,
}

// NOTE: Not sure if I need this
impl ConversationEntity {
    pub fn to_str(&self) -> &'static str {
        match self {
            ConversationEntity::User => "user",
            ConversationEntity::Assistant => "assistant",
        }
    }
}

impl Display for ConversationEntity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConversationEntity::User => write!(f, "User"),
            ConversationEntity::Assistant => write!(f, "Assistant"),
        }
    }
}

// NOTE: USE THE MESSAGE OBJECT - NO NEED TO CREATE MY OWN
#[derive(Debug, Deserialize, Serialize)] 
pub struct Conversation {
    pub role: ConversationEntity,
    pub content: String,
}

impl Conversation {
    pub fn new(
        role: ConversationEntity,
        content: String
    ) -> Conversation {
        Conversation {
            role,
            content,
        }
    }
}

impl Display for Conversation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.role, self.content)
    }
}


#[derive(Debug, Deserialize, Serialize)] 
pub struct Content {
    pub text: String,
}


// NOTE: Either implement Ser De for the Message or just use my own

// #[derive(Debug, Deserialize, Serialize)] 
// pub struct ChTest {
//     pub message: Message,
// }



#[derive(Debug, Deserialize, Serialize)]
pub struct ConversationHistory {
    pub title: Option<String>,
    pub summary: Option<String>,
    pub history: Option<String>,
    pub timestamp: String,
}

impl ConversationHistory {
    pub fn new(
        title: Option<String>,
        summary: Option<String>,
        history: Option<String>,
    ) -> ConversationHistory {
        let local: DateTime<Local> = Local::now(); // e.g. `2014-11-28T21:45:59.324310806+09:00`
        ConversationHistory {
            title,
            summary,
            history,
            timestamp: local.to_string(),
        }
    }

    async fn generate_title(
        &self,
        client: &aws_sdk_bedrockruntime::Client,
    ) -> Result<String, anyhow::Error> {
        let query = constants::CONVERSATION_TITLE_PROMPT.replace(
            "{}", &self.history
                .as_ref()
                .unwrap()
        );
        let model_id = constants::CONVERSATION_HISTORY_MODEL_ID;
        let content = ContentBlock::Text(query);
        println!("⏳ | Generating a new file name for this conversation... ");
        // === RETRY MECHANISM ===
        let max_retries = 3;
        let mut retry_count = 0;
        while retry_count < max_retries {
            match call_converse(
                client,
                model_id.to_string(),
                constants::CONVERSATION_HISTORY_TITLE_INF_PARAMS.clone(),
                content.clone(),
                None,
                false,
            )
            .await
            {
                Ok(response) => {
                    println!("✅ | Done ");
                    return Ok(response)
                }
                Err(e) => {
                    // if an error occurs, print it and retry
                    println!("🔴 | Error: {}", e);
                    retry_count += 1;
                }
            }
            // if we have retried max_retries times, return an error
            if retry_count >= max_retries {
                return Err(anyhow!(
                    "Failed to get a response after {} retries",
                    max_retries
                ));
            }
            // sleep for 2^retry_count seconds - exponential backoff
            tokio::time::sleep(std::time::Duration::from_secs(2u64.pow(retry_count))).await;
            // === END RETRY MECHANISM ===
        }
        Err(anyhow!("Unexpected error in generate_title"))
    }
    async fn generate_summary(
        &self,
        client: &aws_sdk_bedrockruntime::Client,
    ) -> Result<String, anyhow::Error> {
        let query = constants::CONVERSATION_SUMMARY_PROMPT.replace(
            "{}", 
            &self.history
                .as_ref()
                .unwrap()
        );
        let model_id = constants::CONVERSATION_HISTORY_MODEL_ID;
        let content = ContentBlock::Text(query);
        println!("⏳ | Generating a summary for this conversation... ");
        println!();
        // === RETRY MECHANISM ===
        let max_retries = 3;
        let mut retry_count = 0;
        while retry_count < max_retries {
            match call_converse(
                client,
                model_id.to_string(),
                constants::CONVERSATION_HISTORY_INF_PARAMS.clone(),
                content.clone(),
                None,
                false,
            )
            .await
            {
                Ok(response) => return Ok(response),
                Err(e) => {
                    // if an error occurs, print it and retry
                    println!("🔴 | Error: {}", e);
                    retry_count += 1;
                }
            }
            // if we have retried max_retries times, return an error
            if retry_count >= max_retries {
                return Err(anyhow!(
                    "Failed to get a response after {} retries",
                    max_retries
                ));
            }
            // sleep for 2^retry_count seconds - exponential backoff
            tokio::time::sleep(std::time::Duration::from_secs(2u64.pow(retry_count))).await;
            // === END RETRY MECHANISM ===
        }
        Err(anyhow!("Unexpected error in generate_summary"))
    }
}

// TODO: Name the chat histories somehow
pub async fn save_chat_history(
    conversation_history: &str,
    filename: Option<&str>,
    title: Option<String>,
    client: &aws_sdk_bedrockruntime::Client,
) -> Result<String, anyhow::Error> {
    let home_dir = home_dir().expect("Failed to get HOME directory");
    let save_dir = home_dir.join(format!(".config/{}/chats", constants::CONFIG_DIR_NAME));
    fs::create_dir_all(&save_dir)?;

    // some data we need
    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");

    // create ConversationHistory Struct
    let mut ch = ConversationHistory::new(
        if let Some(title) = title {
            Some(title)
        } else {
            Some("title".into())
        },
        Some("summary".to_string()),
        Some(conversation_history.to_string()),
    );

    // generate the conversation summary
    ch.summary = Some(ch.generate_summary(client).await?);

    // if we pass it Some filename - we keep using that file as history
    let (filename, file_path) = if let Some(existing_filename) = filename {
        (
            existing_filename.to_string(),
            save_dir.join(existing_filename),
        )
    } else {
        let title = ch.generate_title(client).await?;
        let new_filename = format!("{}.json", title);
        ch.title = Some(title.clone());
        (new_filename.clone(), save_dir.join(&new_filename))
    };

    // serialize ConversationHistory into a json file
    fs::write(&file_path, serde_json::to_string_pretty(&ch)?)?;

    Ok(filename)
}

pub fn load_chat_history(
    filename: &str,
) -> Result<(String, String, String, String), anyhow::Error> {
    let home_dir = home_dir().expect("Failed to get HOME directory");
    let chat_dir = home_dir.join(format!(".config/{}/chats", constants::CONFIG_DIR_NAME));
    let file_path = chat_dir.join(filename);

    let content = fs::read_to_string(file_path)?;

    let ch = serde_json::from_str::<ConversationHistory>(content.as_str())?;
    Ok((
        ch.history.unwrap(),
        filename.to_string(),
        ch.title.expect("NO_TITLE").to_string(),
        ch.summary.expect("NO_SUMMARY"),
    ))
}

pub fn print_conversation_history(history: &str) {
    const MAX_CHARACTERS_WITHOUT_PROMPT: usize = 1000;

    print_warning("----------------------------------------");
    let confirmation = Confirm::new()
        .with_prompt("Do you want to print the conversation history?")
        .interact()
        .unwrap();

    if confirmation {
        print_warning("----------------------------------------");
        println!("Conversation history: ");
        // check if conversation history is long
        if history.len() > MAX_CHARACTERS_WITHOUT_PROMPT {
            println!(
                "This conversation history is very long ({} characters).",
                history.len()
            );
            print!("Do you want to display the entire history? (y/n): ");
            io::stdout().flush().unwrap();

            let mut user_input = String::new();
            io::stdin().read_line(&mut user_input).unwrap();

            if user_input.trim().to_lowercase() == "y" {
                println!("{}", history.yellow());
            } else {
                println!(
                    "Displaying first {} characters:",
                    MAX_CHARACTERS_WITHOUT_PROMPT
                );
                println!("{}", &history[..MAX_CHARACTERS_WITHOUT_PROMPT].yellow());
                println!("... (truncated)");
            }
        } else {
            println!("{}", history.yellow());
        }
    }
}

pub fn list_chat_histories() -> Result<Vec<String>, anyhow::Error> {
    let home_dir = home_dir().expect("Failed to get HOME directory");
    let chat_dir = home_dir.join(format!(".config/{}/chats", constants::CONFIG_DIR_NAME));

    let mut chat_files = Vec::new();
    for entry in fs::read_dir(chat_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("json") {
            if let Some(filename) = path.file_name().and_then(|s| s.to_str()) {
                chat_files.push(filename.to_string());
            }
        }
    }

    chat_files.sort_by(|a, b| b.cmp(a)); // Sort in descending order (newest first)
    Ok(chat_files)
}
