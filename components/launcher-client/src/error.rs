use habitat_launcher_protocol as protocol;
use ipc_channel::ipc::IpcError;
use std::{error,
          fmt,
          io,
          result};

#[derive(Debug)]
#[allow(clippy::upper_case_acronyms)]
pub enum Error {
    AcceptConn,
    BadPipe(io::Error),
    Connect(io::Error),
    IPCBincode(String),
    IPCIO(IpcError),
    Protocol(protocol::Error),
    Send(ipc_channel::Error),
    Timeout,
}

pub type Result<T> = result::Result<T, Error>;

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = match *self {
            Error::AcceptConn => "Unable to accept connection from Launcher".to_string(),
            Error::BadPipe(ref e) => format!("Unable to open pipe to Launcher, {}", e),
            Error::Connect(ref e) => format!("Unable to connect to Launcher's pipe, {}", e),
            Error::IPCBincode(ref e) => {
                format!("Unable to read message frame from Launcher, {}", e)
            }
            Error::IPCIO(ref e) => format!("Unable to receive message from Launcher, {:?}", e),
            Error::Protocol(ref e) => format!("{}", e),
            Error::Send(ref e) => format!("Unable to send to Launcher's pipe, {}", e),
            Error::Timeout => "Launcher interaction timed out".to_string(),
        };
        write!(f, "{}", msg)
    }
}

impl error::Error for Error {}

impl From<IpcError> for Error {
    fn from(err: IpcError) -> Error {
        match err {
            IpcError::Bincode(err) => Error::IPCBincode(err.to_string()),
            _ => Error::IPCIO(err)
        }
    }
}

impl From<protocol::Error> for Error {
    fn from(err: protocol::Error) -> Error { Error::Protocol(err) }
}
