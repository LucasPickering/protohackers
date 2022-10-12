use crate::{
    error::{ServerError, ServerResult},
    problems::TcpServer,
    util::socket_write,
};
use anyhow::Context;
use async_trait::async_trait;
use derive_more::Display;
use log::info;
use std::collections::HashSet;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::TcpStream,
    sync::{
        broadcast,
        broadcast::{Receiver, Sender},
        RwLock,
    },
};

#[derive(Debug)]
pub struct ChatServer {
    /// broadcast (i.e. multi-producer/multi-consumer) channel sender
    sender: Sender<ChatMessage>,
    /// We need to hold this to keep the channel open. If all receivers are
    /// dropped, it will close.
    _receiver: Receiver<ChatMessage>,
    /// List of active users, so we can print it out every time someone joins
    active_users: RwLock<HashSet<Username>>,
}

impl ChatServer {
    pub fn new() -> Self {
        // Problem statement requires 10 simulatenous clients
        let (sender, receiver) = broadcast::channel(10);
        Self {
            sender,
            _receiver: receiver,
            active_users: RwLock::new(HashSet::new()),
        }
    }

    /// Broadcast a message to all other users
    fn broadcast(&self, message: ChatMessage) -> ServerResult<()> {
        info!("{}", message);
        self.sender.send(message).context("sending chat message")?;
        Ok(())
    }

    async fn join(
        &self,
        username: Username,
        socket_writer: impl AsyncWriteExt + Unpin,
    ) -> ServerResult<Receiver<ChatMessage>> {
        // Print out who is already here
        let roster_message = {
            // It's important that this is scoped, so we drop the read ASAP to
            // unblock potential writes on other tasks.
            let users = self.active_users.read().await;
            format!(
                "* present and accounted for: {}\n",
                users
                    .iter()
                    .map(Username::as_str)
                    .collect::<Vec<&str>>()
                    .join(", "),
            )
        };
        socket_write(socket_writer, roster_message.as_bytes()).await?;

        // Let everyone else know we're here
        self.broadcast(ChatMessage::Join {
            username: username.clone(),
        })?;

        // Add new user to the list
        {
            let mut users = self.active_users.write().await;
            users.insert(username);
        }

        // Now that the user is fully joined, they can start listening to the
        // broadcast
        Ok(self.sender.subscribe())
    }

    async fn leave(&self, username: Username) -> ServerResult<()> {
        // It shouldn't matter which order we do these two operations in.
        // Hypothetically it affects race conditions, but the difference is
        // almost definitely inconsequential

        // Slam the door on our way out so everyone knows we left
        self.broadcast(ChatMessage::Leave {
            username: username.clone(),
        })?;

        // Remove user from the list
        let mut users = self.active_users.write().await;
        users.remove(&username);

        Ok(())
    }
}

#[async_trait]
impl TcpServer for ChatServer {
    async fn handle_client(&self, socket: TcpStream) -> ServerResult<()> {
        // Split the stream into buffered reader+writer so we can do
        // line-delimited operations
        let (socket_reader, mut socket_writer) = socket.into_split();
        let socket_reader = BufReader::new(socket_reader);
        let mut lines = socket_reader.lines();

        // Start by asking for their username
        socket_write(&mut socket_writer, b"new phone who dis?\n").await?;
        let username = match lines.next_line().await? {
            Some(username) if Username::is_valid(&username) => {
                Username(username)
            }
            // Either the username was invalid or the user disconnected. Either
            // way, just bail
            _ => return Err(ServerError::SocketClose),
        };

        // Add the user to the channel
        let mut receiver =
            self.join(username.clone(), &mut socket_writer).await?;

        // We now need to run communications in two directions:
        // - Broadcast listener => TCP sender
        // - TCP listener => broadcast sender
        // We can only do one of these in this task, so the other needs to be
        // spawned off into a subtask

        // Create a subtask to listen to broadcast messages from other users.
        // Each time we hear one, we'll forward it to the client via TCP
        let task_owner = username.clone();
        let receiver_handle = tokio::spawn(async move {
            while let Ok(message) = receiver.recv().await {
                // Ignore messages from ourselves
                if message.username() != &task_owner {
                    let mut message_string = message.to_string();
                    message_string.push('\n');
                    if socket_write(
                        &mut socket_writer,
                        message_string.as_bytes(),
                    )
                    .await
                    .is_err()
                    {
                        // Ignore the error, since I'm lazy
                        return;
                    }
                }
            }
        });

        // Listen for messages from the client, and broadcast to other clients
        while let Some(line) = lines.next_line().await? {
            self.broadcast(ChatMessage::Chat {
                sender: username.clone(),
                content: line,
            })?;
        }

        // Stop listening for broadcast, then wait until the task dies
        receiver_handle.abort();
        // This is *always* going to fail since we aborted, so ignore it
        receiver_handle.await.ok();

        // unsubscribed
        self.leave(username).await?;

        Ok(())
    }
}

/// Newtype wrapper for usernames
#[derive(Clone, Debug, Display, Eq, Hash, PartialEq)]
struct Username(String);

impl Username {
    /// Check if the username is valid, according to the problem definition
    fn is_valid(username: &str) -> bool {
        !username.is_empty()
            && username.len() <= 16
            && username.chars().all(char::is_alphanumeric)
    }

    fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

/// A message propagated over the chat channel
#[derive(Clone, Debug, Display)]
enum ChatMessage {
    #[display(fmt = "* {} has entered the room", "username")]
    Join { username: Username },
    #[display(fmt = "* {} has left the room", "username")]
    Leave { username: Username },
    #[display(fmt = "[{}] {}", "sender", "content")]
    Chat { sender: Username, content: String },
}

impl ChatMessage {
    /// Get the username of the person who sent the chat message
    fn username(&self) -> &Username {
        match self {
            Self::Join { username } => username,
            Self::Leave { username } => username,
            Self::Chat { sender, .. } => sender,
        }
    }
}
