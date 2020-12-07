use crate::time::StdDuration;
use anyhow::{Context, Result};
use async_trait::async_trait;
use futures::future::poll_fn;
use std::{
    env::VarError,
    ops::Deref,
    str::FromStr,
    sync::{Arc, Weak},
};
use tokio::{sync::Mutex, task::JoinHandle};
use tokio_postgres::{
    tls::NoTlsStream, AsyncMessage, Client, Connection, IsolationLevel, NoTls, Socket,
    Transaction,
};

pub struct Dummy;

#[async_trait]
pub trait FromClient: Sync + Send + Sized + 'static {
    async fn from_client(client: &Client) -> Result<Self>;
}

#[async_trait]
impl FromClient for Dummy {
    async fn from_client(_client: &Client) -> Result<Self> {
        Ok(Self)
    }
}

/// Container for a single postgres connection
///
/// This container implements a kind of MVCC. Users who borrow the connection get access
/// to the connection that was contained in the container at the time. If the contained
/// connection is replaced, previous users will hold on to the version they borrowed.
///
/// Multiple users can use the contained connection concurrently. Are queries are
/// pipelined.
///
/// If the connection fails, it will get replaced by a new connection the next time
/// someone tries to borrow it. This operation is transparent.
pub struct PgHolder<T = Dummy, R = NoOpMessageHandler> {
    con: Mutex<(u64, Option<Arc<Pg<T>>>, Option<JoinHandle<()>>)>,
    message_handler: R,
    persistent: bool,
}

pub type PgClient = Client;

/// A postgres connection with associated data
///
/// The associated data of type `T` can be used to prepare statements when the connection
/// is established. See [site::db::Statements].
pub struct Pg<T> {
    client: PgClient,
    pub t: T,
}

impl<T> Deref for Pg<T> {
    type Target = PgClient;

    fn deref(&self) -> &Self::Target {
        &self.client
    }
}

/// Creates a transaction with isolation level repeatable read
pub async fn transaction<'a>(con: &'a mut PgClient) -> Result<Transaction<'a>> {
    Ok(con
        .build_transaction()
        .isolation_level(IsolationLevel::RepeatableRead)
        .start()
        .await?)
}

pub const PG_APPLICATION_NAME: &str = "PG_APPLICATION_NAME";

/// Set the application name of connections established by this process
pub fn set_name(name: &str) {
    std::env::set_var(PG_APPLICATION_NAME, name);
}

async fn connect_raw() -> Result<(PgClient, Connection<Socket, NoTlsStream>)> {
    const PG_CONNECTION_STRING: &str = "PG_CONNECTION_STRING";
    let connection_string = match std::env::var(PG_CONNECTION_STRING) {
        Ok(s) => s,
        Err(VarError::NotPresent) => {
            panic!("{} environment variable is not set", PG_CONNECTION_STRING)
        }
        Err(VarError::NotUnicode(_)) => panic!("{} is not Unicode", PG_CONNECTION_STRING),
    };
    let (client, con) = {
        let mut config = tokio_postgres::Config::from_str(&connection_string)?;
        if let Ok(s) = std::env::var(PG_APPLICATION_NAME) {
            config.application_name(&s);
        }
        config
            .connect(NoTls)
            .await
            .context("cannot connect to postgres")?
    };
    Ok((client, con))
}

/// Creates a new postgres client
pub async fn connect() -> Result<PgClient> {
    connect_with_handler(&NoOpMessageHandler)
        .await
        .map(|(a, _)| a)
}

/// Creates a new postgres client with a message handler
async fn connect_with_handler<M: MessageHandler>(
    message_handler: &M,
) -> Result<(PgClient, JoinHandle<()>)> {
    let (client, con) = connect_raw().await?;
    let handler2 = message_handler.clone();
    let join_handle = tokio::spawn(async move {
        if let Err(e) = drive_connection(con, handler2).await {
            log::error!("postgres connection failed: {:#}", e);
        }
    });
    message_handler.listen(&client).await?;
    Ok((client, join_handle))
}

/// Creates a postgres client with associated data
async fn client<T: FromClient, M: MessageHandler>(
    message_handler: &M,
) -> Result<(Pg<T>, JoinHandle<()>)> {
    let (client, join_handle) = connect_with_handler(message_handler).await?;
    let pg = Pg {
        t: T::from_client(&client).await?,
        client,
    };
    Ok((pg, join_handle))
}

impl<T: FromClient> PgHolder<T> {
    /// Creates a new container
    pub fn new() -> Arc<Self> {
        Self::with_message_handler(NoOpMessageHandler, false)
    }
}

impl<T: FromClient, M: MessageHandler> PgHolder<T, M> {
    /// Creates a new container
    pub fn with_message_handler(message_handler: M, persistent: bool) -> Arc<Self> {
        let holder = Arc::new(Self {
            con: Mutex::new((0, None, None)),
            message_handler,
            persistent,
        });
        if persistent {
            tokio::spawn(keep_connected(Arc::downgrade(&holder)));
        }
        holder
    }

    /// Borrows the connection
    pub async fn borrow(&self) -> Result<Arc<Pg<T>>> {
        loop {
            let (ver, con) = {
                let locked = self.con.lock().await;
                (locked.0, locked.1.clone())
            };
            if let Some(con) = con {
                if con.simple_query("").await.is_ok() {
                    return Ok(con);
                }
            }
            self.connect(ver).await?;
        }
    }

    async fn connect(&self, ver: u64) -> Result<()> {
        let mut locked = self.con.lock().await;
        if ver == locked.0 {
            log::info!(
                "creating postgres connection for thread {}",
                std::thread::current().name().unwrap_or("?")
            );
            let (client, join_handle) = client(&self.message_handler).await?;
            locked.0 = ver + 1;
            locked.1 = Some(Arc::new(client));
            if self.persistent {
                locked.2 = Some(join_handle);
            }
        }
        Ok(())
    }
}

async fn keep_connected<T: FromClient, R: MessageHandler>(holder: Weak<PgHolder<T, R>>) {
    while let Some(holder) = holder.upgrade() {
        let join_handle = {
            let (ver, join_handle) = {
                let mut locked = holder.con.lock().await;
                (locked.0, locked.2.take())
            };
            match join_handle {
                Some(h) => h,
                _ => {
                    if let Err(e) = holder.connect(ver).await {
                        log::error!("could not connect to postgres: {:#}", e);
                        log::info!("sleeping for 10 seconds");
                        drop(holder);
                        tokio::time::delay_for(StdDuration::from_secs(10)).await;
                    }
                    continue;
                }
            }
        };
        drop(holder);
        let _ = join_handle.await;
    }
}

async fn drive_connection<T: MessageHandler>(
    mut con: Connection<Socket, NoTlsStream>,
    handler: T,
) -> Result<()> {
    loop {
        let pf = poll_fn(|cx| con.poll_message(cx));
        let message = match pf.await.transpose()? {
            Some(m) => m,
            _ => return Ok(()),
        };
        match message {
            AsyncMessage::Notice(notice) => {
                log::info!("{}: {}", notice.severity(), notice.message());
            }
            AsyncMessage::Notification(notification) => {
                handler.handle(notification.channel(), notification.payload());
            }
            _ => log::warn!("received unknown async postgres message"),
        }
    }
}

#[async_trait]
pub trait MessageHandler: Clone + Send + Sync + 'static {
    async fn listen(&self, client: &PgClient) -> Result<()>;
    fn handle(&self, channel: &str, payload: &str);
}

#[derive(Clone)]
pub struct NoOpMessageHandler;

#[async_trait]
impl MessageHandler for NoOpMessageHandler {
    async fn listen(&self, _client: &PgClient) -> Result<()> {
        Ok(())
    }

    fn handle(&self, _channel: &str, _payload: &str) {
        // nothing
    }
}

/// Creates a """typed""" prepared statement
///
/// The use of this is that it
///
/// - guarantees that the fields you want to access are actually returned by the query
/// - calculates the indices of the fields once when the statement is prepared
/// - allows you to access that field by identifier
///
/// # Example
///
/// ```no_run
/// common::create_statement!(ShowsStmt, show_id, name, show_name_type; "
///     select show_id, name, show_name_type
///     from magnets.show_name
///     where show_name_type in (1, 2)");
/// ```
///
/// Expands to
///
/// ```no_run
/// # use anyhow::Result;
/// pub struct ShowsStmt {
///     pub stmt: tokio_postgres::Statement,
///     pub show_id: usize,
///     pub name: usize,
///     pub show_name_type: usize,
/// }
/// impl ShowsStmt {
///     async fn new<T: tokio_postgres::GenericClient>(con: &T) -> Result<Self> {
///         let stmt = con.prepare("
///     select show_id, name, show_name_type
///     from magnets.show_name
///     where show_name_type in (1, 2)").await?;
///         let mut show_id = None;
///         let mut name = None;
///         let mut show_name_type = None;
///         for (idx, col) in stmt.columns().iter().enumerate() {
///             match col.name() {
///                 stringify!( show_id ) => show_id = Some(idx),
///                 stringify!( name ) => name = Some(idx),
///                 stringify!( show_name_type ) => show_name_type = Some(idx),
///                 _ => {}
///             }
///         }
///         Ok(Self {
///             stmt,
///             show_id: show_id.unwrap(),
///             name: name.unwrap(),
///             show_name_type: show_name_type.unwrap(),
///         })
///     }
/// }
/// ```
#[macro_export]
macro_rules! create_statement {
    ($name:ident $(,$field:ident)*; $stmt:expr) => {
        pub struct $name {
            pub stmt: tokio_postgres::Statement,
            $(
                pub $field: usize,
            )*
        }

        impl $name {
            async fn new<T: tokio_postgres::GenericClient>(con: &T) -> Result<Self> {
                let stmt = con.prepare($stmt).await?;
                $(
                    let mut $field = None;
                )*
                for (idx, col) in stmt.columns().iter().enumerate() {
                    match col.name() {
                        $(
                            stringify!($field) => $field = Some(idx),
                        )*
                        _ => { },
                    }
                }
                Ok(Self {
                    stmt,
                    $(
                        $field: $field.unwrap(),
                    )*
                })
            }
        }
    }
}
