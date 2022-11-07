mod decode;
mod encode;

use crate::{
    error::{ServerError, ServerResult},
    problems::{
        problem6::decode::{Decoder, ParseOutcome},
        TcpServer,
    },
    util::ResultExt,
};
use anyhow::Context;
use async_trait::async_trait;
use derive_more::Display;
use futures::{pin_mut, StreamExt};
use log::{debug, error, info, trace, warn};
use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};
use thiserror::Error;
use tokio::{
    io::AsyncWriteExt,
    net::{tcp::OwnedWriteHalf, TcpStream},
    sync::{
        mpsc::{self, Sender},
        RwLock,
    },
    task::JoinHandle,
    time,
};

#[derive(Debug)]
pub struct SpeedDaemonServer {
    /// For each road, we'll keep a receptacle of tickets. This will be
    /// populated lazily, either as a ticket is reported or a dispatcher
    /// connects.
    ticket_boxes: RwLock<HashMap<Road, TicketBox>>,
    /// For each plate+road pairing, we store the list of timestamped
    /// observations, **sorted chronologically**. All modifications to this
    /// should maintain ordering.
    plate_observations: RwLock<HashMap<(Plate, Road), Vec<Observation>>>,
}

impl SpeedDaemonServer {
    pub fn new() -> Self {
        Self {
            ticket_boxes: RwLock::default(),
            plate_observations: RwLock::default(),
        }
    }

    /// Handle an incoming client message
    async fn handle_message(
        &self,
        client: &mut Client,
        message: ClientMessage,
    ) -> SpeedResult<()> {
        match message {
            ClientMessage::Plate { plate, timestamp } => {
                let camera = client.get_camera()?;
                self.observe_plate(camera, plate, timestamp).await;
            }
            ClientMessage::WantHeartbeat { interval } => {
                client.start_heartbeat(interval)?;
            }
            ClientMessage::IAmCamera { camera } => {
                client.identify(ClientType::Camera(camera))?;
            }
            ClientMessage::IAmDispatcher { roads } => {
                // Register the dispatcher for all of its constituent roads
                client.identify(ClientType::Dispatcher)?;
                self.add_dispatcher(client.get_sender(), &roads).await?;
            }
        };
        Ok(())
    }

    /// Add a dispatcher to one or more roads. For each road in the list, the
    /// dispatcher will be added to that road's roster.
    async fn add_dispatcher(
        &self,
        sender: Sender<ServerMessage>,
        roads: &[Road],
    ) -> SpeedResult<()> {
        let mut dispatchers = self.ticket_boxes.write().await;
        for road in roads {
            let ticket_box = dispatchers.entry(*road).or_default();
            ticket_box.set_dispatcher(sender.clone()).await?;
        }
        Ok(())
    }

    async fn observe_plate(
        &self,
        camera: Camera,
        plate: Plate,
        timestamp: Timestamp,
    ) {
        let key = (plate.clone(), camera.road);

        // First, check if this new observation warrants a ticket by comparing
        // against all other observations. Important: observations aren't
        // necessaryily reported in order, so we need to check *all* existing
        // observations, not just earlier ones
        let observation = Observation {
            timestamp,
            mile: camera.mile,
        };

        // Lexical scopes below are to prevent deadlocks. Theoretically NLL
        // should save us here, but better safe than sorry
        {
            // See if we've see this plate on this road already today
            let observations_map = self.plate_observations.read().await;
            let previous_observations =
                observations_map.get(&key).map(Vec::as_slice).unwrap_or(&[]);
            debug!(
                "Observed {} on {}:{} @ {}, \
                    comparing to {} previous observations",
                plate,
                camera.road,
                camera.mile,
                timestamp,
                previous_observations.len()
            );
            for &other_observation in previous_observations {
                let speed = observation.get_speed(other_observation);
                if speed > camera.speed_limit * 100 {
                    self.report_speeding(
                        plate.clone(),
                        camera.road,
                        observation,
                        other_observation,
                        speed,
                    )
                    .await;
                }
            }
        }

        // Store this observation. We need to do this all while holding the
        // write lock, to prevent race conditions leading to incorrect insert
        // index.
        {
            let mut observations_map = self.plate_observations.write().await;
            let observations = observations_map.entry(key).or_default();
            let insert_index = observations
                .binary_search_by_key(&timestamp, |obs| obs.timestamp)
                .smush();
            observations.insert(insert_index, observation);
        }
    }

    async fn report_speeding(
        &self,
        plate: Plate,
        road: Road,
        observation1: Observation,
        observation2: Observation,
        speed: u16,
    ) {
        let mut dispatchers = self.ticket_boxes.write().await;
        // Make sure the earlier observation is first!
        let (observation1, observation2) =
            if observation1.timestamp < observation2.timestamp {
                (observation1, observation2)
            } else {
                (observation2, observation1)
            };

        // Report the ticket. The ticket box will decide if this can be
        // dispatched now or saved for later. If the car has already received
        // a ticket today, then don't send it.
        let ticket = Ticket {
            plate,
            road,
            observation1,
            observation2,
            speed,
        };
        let ticket_box = dispatchers.entry(road).or_default();
        if !ticket_box.is_duplicate(&ticket) {
            ticket_box.send_ticket(ticket).await;
        } else {
            info!("Skipping repeat ticket {:?}", ticket);
        }
    }
}

#[async_trait]
impl TcpServer for SpeedDaemonServer {
    async fn handle_client(
        &self,
        client_stream: TcpStream,
    ) -> ServerResult<()> {
        let (reader, writer) = client_stream.into_split();
        let mut client = Client::new(writer);
        let mut decoder = Decoder::new(reader);

        // Read messages from the socket, until it hits EOF or a terminal error.
        let message_stream = decoder.read_messages();
        pin_mut!(message_stream);
        while let Some(parse_outcome) =
            message_stream.next().await.transpose()?
        {
            // We'll handle errors from receiving and handling at the same time
            match parse_outcome {
                ParseOutcome::Success(message) => {
                    match self.handle_message(&mut client, message).await {
                        Ok(()) => {}
                        // Unexpected server error occurred, kill the connection
                        Err(SpeedError::ServerError(error)) => {
                            return Err(error)
                        }
                        // Client-induced error occurred, report it to them
                        Err(error) => {
                            client
                                .send(ServerMessage::Error {
                                    message: error.to_string(),
                                })
                                .await?
                        }
                    }
                }
                ParseOutcome::IllegalMessageType => {
                    client
                        .send(ServerMessage::Error {
                            message: SpeedError::IllegalMessageType.to_string(),
                        })
                        .await?
                }
                // Unexpected server error occurred, kill the connection
                ParseOutcome::UnknownError(error) => return Err(error.into()),
            }
        }
        Ok(())
    }
}

type Plate = String;
type Road = u16;
type Mile = u16;
type SpeedLimit = u16;

#[derive(Clone, Debug)]
enum ServerMessage {
    Error { message: String },
    Ticket { ticket: Ticket },
    Heartbeat,
}

#[derive(Clone, Debug)]
enum ClientMessage {
    Plate {
        plate: Plate,
        timestamp: Timestamp,
    },
    WantHeartbeat {
        /// Interval is in deciseconds (100ms)
        interval: u32,
    },
    IAmCamera {
        camera: Camera,
    },
    IAmDispatcher {
        roads: Vec<u16>,
    },
}

#[derive(Copy, Clone, Debug, Display, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct Timestamp(u32);

impl Timestamp {
    /// Get the ordinal number representing the day that this timestamp falls on
    fn day(self) -> u32 {
        self.0 / 86400
    }
}

#[derive(Copy, Clone, Debug)]
struct Camera {
    road: Road,
    mile: Mile,
    speed_limit: SpeedLimit,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct Ticket {
    plate: Plate,
    road: Road,
    observation1: Observation,
    observation2: Observation,
    /// Speed is in **100x MPH***
    speed: u16,
}

impl Ticket {
    /// Get the list of days that this ticket spans. Typically 1, but could be
    /// a lot more
    fn days(&self) -> impl Iterator<Item = u32> {
        self.observation1.timestamp.day()..=self.observation2.timestamp.day()
    }
}

/// A receptacle for tickets. This can either be connected to a particular
/// dispatcher (via an mpsc channel, which will forward messages over TCP),
/// or a queue that will hold onto tickets until a dispatcher connects.
#[derive(Debug, Default)]
struct TicketBox {
    /// A list of all tickets registered on this road, with a flag denoting
    /// whether or not the ticket has been dispatcher
    tickets: HashMap<Ticket, bool>,
    dispatcher: Option<Sender<ServerMessage>>,
}

impl TicketBox {
    /// Get the list of all tickets that have yet to be sent
    fn unsent_tickets(&self) -> impl Iterator<Item = &Ticket> {
        self.tickets
            .iter()
            .filter(|(_, sent)| !*sent)
            .map(|(ticket, _)| ticket)
    }

    /// Check if a ticket has already been reported on the same day as the given
    /// one. A ticket can span multiple days, so this will check for all
    /// possible overlaps between the two.
    fn is_duplicate(&self, ticket: &Ticket) -> bool {
        let days: HashSet<_> = ticket.days().collect();
        // For each existing ticket, check if any days overlap with the new one
        self.tickets
            .keys()
            .filter(|other| other.plate == ticket.plate)
            .any(|other| !days.is_disjoint(&other.days().collect()))
    }

    /// Set the dispatcher for this ticket box. If there are unsent tickets
    /// in the queue, we'll send those now.
    ///
    /// If the road is already managed by another dispatcher, then we'll
    /// just take over. The problem statement says that if multiple
    /// dispatchers are registered for a road, we should send the ticket
    /// to just one,and choose arbitrarily. Rather than saving all
    /// dispatchers andchoosing one at ticket time, we should just save
    /// the most recentone to register and boot any others out. Less
    /// work, same result.
    async fn set_dispatcher(
        &mut self,
        dispatcher: Sender<ServerMessage>,
    ) -> ServerResult<()> {
        // Dispatch all unsent tickets now
        for ticket in self.unsent_tickets() {
            debug!("Dispatching backlog ticket {:?}", ticket);
            Self::dispatch_ticket(&dispatcher, ticket.clone()).await?;
        }
        // Register the dispatcher. We do this second so we can pass a ref to
        // the dispatcher above
        self.dispatcher = Some(dispatcher);
        Ok(())
    }

    /// Add a ticket to the box. The ticket will be stored for posterity, and
    /// dispatched if there is a dispatcher present. This will *not* eat
    /// duplicate tickets, so you need to check for dupes before calling
    /// this method!!
    async fn send_ticket(&mut self, ticket: Ticket) {
        let sent = match &self.dispatcher {
            Some(dispatcher) => {
                debug!("Dispatching new ticket {:?}", ticket);
                // If dispatching fails, the dispatcher has disconnected. We
                // should still store the ticket, but wipe out the dispatcher
                // since they're not coming back.
                match Self::dispatch_ticket(dispatcher, ticket.clone()).await {
                    Ok(()) => true,
                    Err(error) => {
                        error!("Error dispatching ticket, dispatcher disconnected? {}", error);
                        self.dispatcher = None;
                        false
                    }
                }
            }
            None => {
                debug!("Backlogging ticket {:?}", ticket);
                false
            }
        };
        // Store the ticket. Flag indicates whether or not it's been dispatched.
        self.tickets.insert(ticket, sent);
    }

    /// Send a ticket to a dispatcher. This is a minimal function and shouldn't
    /// be called directly from outside the struct. Use [Self::send_ticket]
    /// instead.
    async fn dispatch_ticket(
        dispatcher: &Sender<ServerMessage>,
        ticket: Ticket,
    ) -> ServerResult<()> {
        let message = ServerMessage::Ticket { ticket };
        dispatcher
            .send(message)
            .await
            .context("Error dispatching ticket")?;
        Ok(())
    }
}

#[derive(Debug)]
enum ClientType {
    Camera(Camera),
    Dispatcher,
}

/// A container for a connected client. The client will typically be identified,
/// meaning it's told us whether it's a camera or a dispatcher. This also holds
/// onto the socket writer, as well as an mpsc channel. The channel will collect
/// server messages from multiple subtasks, then funnel those all into the
/// socket with a subtask of its own
#[derive(Debug)]
struct Client {
    client_type: Option<ClientType>,
    /// This is the proto sender, which we'll just use to clone
    sender: Sender<ServerMessage>,
    funnel_handle: JoinHandle<()>,
    heartbeat_handle: Option<JoinHandle<()>>,
}

impl Client {
    fn new(mut stream_writer: OwnedWriteHalf) -> Self {
        let (sender, mut receiver) = mpsc::channel::<ServerMessage>(16);

        // Spawn a task to funnel all messages on the mpsc channel into the TCP
        // socket
        let peer_addr = stream_writer.peer_addr().unwrap();
        let funnel_handle = tokio::spawn(async move {
            while let Some(message) = receiver.recv().await {
                let bytes = message.encode();
                info!("{} <= {:?} {:02x?}", peer_addr, message, bytes);
                if let Err(error) = stream_writer.write_all(&bytes).await {
                    error!("Error writing message to socket: {}", error);
                }
            }
        });

        Self {
            client_type: None,
            sender,
            funnel_handle,
            heartbeat_handle: None,
        }
    }

    /// Set the identity of this client. If the identity is already set, return
    /// an error.
    fn identify(&mut self, client: ClientType) -> SpeedResult<()> {
        match &self.client_type {
            None => {
                self.client_type = Some(client);
                Ok(())
            }
            Some(_) => Err(SpeedError::AlreadyIdentified),
        }
    }

    /// Get the contained camera, or error if this client isn't a camera
    fn get_camera(&self) -> SpeedResult<Camera> {
        match self.client_type.as_ref() {
            Some(ClientType::Camera(camera)) => Ok(*camera),
            _ => Err(SpeedError::NotACamera),
        }
    }

    /// Get an mpsc sender, which can be used to send server messages to this
    /// client.
    fn get_sender(&self) -> Sender<ServerMessage> {
        self.sender.clone()
    }

    /// Send a message to the client over the socket. The message will first
    /// go through the mpsc channel, then funneled into the socket.
    async fn send(&mut self, message: ServerMessage) -> ServerResult<()> {
        Ok(self
            .sender
            .send(message)
            .await
            .context("Error sending mpsc message")?)
    }

    fn start_heartbeat(&mut self, interval_ds: u32) -> SpeedResult<()> {
        if self.heartbeat_handle.is_some() {
            return Err(SpeedError::AlreadyBeating);
        }

        if interval_ds > 0 {
            // Create another mpsc sender that this subtask can use to send
            // messages
            let sender = self.get_sender();
            let heartbeat_handle = tokio::spawn(async move {
                let mut interval = time::interval(Duration::from_millis(
                    interval_ds as u64 * 100,
                ));
                loop {
                    interval.tick().await;
                    // This only fails if the receiver is closed, at which point
                    // we should just terminate
                    if sender.send(ServerMessage::Heartbeat).await.is_err() {
                        break;
                    }
                }
            });
            self.heartbeat_handle = Some(heartbeat_handle);
        }

        Ok(())
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        // Kill subtasks when client is dropped
        self.funnel_handle.abort();
        if let Some(heartbeat_handle) = &self.heartbeat_handle {
            heartbeat_handle.abort();
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, Hash, PartialEq)]
struct Observation {
    mile: Mile,
    timestamp: Timestamp,
}

impl Observation {
    /// Get speed, in **100x MPH**
    fn get_speed(self, other: Observation) -> u16 {
        // Do all the math as floats to prevent rounding errors
        let mileage = self.mile.abs_diff(other.mile) as f32;
        let seconds_elapsed =
            self.timestamp.0.abs_diff(other.timestamp.0) as f32;
        if seconds_elapsed == 0.0 {
            // This *shouldn't* every happen, but maybe the client tries to
            // trick us
            warn!(
                "0 seconds elapsed between observations can't calculate speed; {:?} <=> {:?}",
                self, other
            );
            0
        } else {
            let mph = (mileage) / seconds_elapsed * 60.0 * 60.0;
            trace!("{}mi / {}s = {}mph", mileage, seconds_elapsed, mph);
            // Problem statement says it's fine to assume speeds are under
            // 655mph, which would overflow the u16 at 100x
            (mph * 100.0) as u16
        }
    }
}

/// An error during server operation. The error can be of a known client error
/// class, in which case it will be gracefully reported to the client. Or it
/// could be a general server error, in which case the connection will close.
#[derive(Debug, Error)]
enum SpeedError {
    #[error("Illegal message type")]
    IllegalMessageType,
    #[error("Client already identified")]
    AlreadyIdentified,
    #[error("Observations can only be reported by cameras")]
    NotACamera,
    #[error("Heartbeat already running")]
    AlreadyBeating,
    /// Server-level errors, should be propagated up
    #[error(transparent)]
    ServerError(#[from] ServerError),
}

impl From<anyhow::Error> for SpeedError {
    fn from(value: anyhow::Error) -> Self {
        Self::ServerError(value.into())
    }
}

type SpeedResult<T> = Result<T, SpeedError>;
