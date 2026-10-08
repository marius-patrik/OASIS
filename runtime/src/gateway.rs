//! Loopback-only development transport for the universal MMO coordinator.
//! It is intentionally NOT a public-network protocol: transport encryption,
//! token revocation, persistence, versioned wire encoding and bandwidth-aware
//! replication are required before external deployment.

use std::io::{self, BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use oasis_contracts::Id;
use crate::universe::{text_intent, SessionAuthority, SessionTicket, Universe};

type SharedUniverse = Arc<Mutex<Universe>>;

pub struct LoopbackGateway {
    address: SocketAddr,
    stop: Arc<AtomicBool>,
    accept: Option<JoinHandle<()>>,
}

impl LoopbackGateway {
    pub fn bind(
        universe: SharedUniverse, authority: Arc<dyn SessionAuthority>,
    ) -> io::Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        let address = listener.local_addr()?;
        listener.set_nonblocking(true)?;
        let stop = Arc::new(AtomicBool::new(false));
        let active = Arc::clone(&stop);
        let accept = thread::spawn(move || {
            while !active.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let shared = Arc::clone(&universe);
                        let auth = Arc::clone(&authority);
                        let state = Arc::clone(&active);
                        thread::spawn(move || {
                            let _ = serve_connection(stream, shared, auth, state);
                        });
                    }
                    Err(err) if err.kind() == io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(_) => break,
                }
            }
        });
        Ok(Self { address, stop, accept:Some(accept) })
    }

    pub fn local_addr(&self) -> SocketAddr { self.address }
}

impl Drop for LoopbackGateway {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(handle) = self.accept.take() {
            let _ = handle.join();
        }
    }
}

fn response(stream: &mut TcpStream, line: &str) -> io::Result<()> {
    stream.write_all(line.as_bytes())?;
    stream.write_all(b"\n")?;
    stream.flush()
}

fn serve_connection(
    mut stream: TcpStream,
    universe: SharedUniverse,
    authority: Arc<dyn SessionAuthority>,
    stop: Arc<AtomicBool>,
) -> io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_millis(200)))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut ticket: Option<SessionTicket> = None;
    let mut line = String::new();
    while !stop.load(Ordering::Relaxed) {
        line.clear();
        let length = match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(n) => n,
            Err(err) if matches!(err.kind(),
                io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut) => continue,
            Err(err) => return Err(err),
        };
        if length > 4096 {
            response(&mut stream,"ERR input-too-long")?;
            break;
        }
        let command = line.trim_end_matches(['\r','\n']);
        let (verb, arg) = command.split_once(' ').unwrap_or((command, ""));
        match verb {
            "PING" => response(&mut stream,"PONG")?,
            "AUTH" if ticket.is_none() && !arg.is_empty() => {
                let outcome = universe.lock()
                    .map_err(|_| io::Error::other("universe lock poisoned"))?
                    .connect(authority.as_ref(),arg);
                match outcome {
                    Ok(handle) => { ticket = Some(handle); response(&mut stream,"OK auth")?; }
                    Err(_) => response(&mut stream,"ERR unauthorized")?,
                }
            }
            "AUTH" => response(&mut stream,"ERR auth-invalid")?,
            "QUIT" => { response(&mut stream,"OK bye")?; break; }
            _ => {
                let Some(session) = ticket else {
                    response(&mut stream,"ERR auth-required")?;
                    continue;
                };
                let mut shared = universe.lock()
                    .map_err(|_| io::Error::other("universe lock poisoned"))?;
                match verb {
                    "JOIN" => {
                        match arg.parse::<u128>() {
                            Ok(id) if shared.enter_world(session,Id(id)).is_ok() => {
                                response(&mut stream,"OK joined")?;
                            }
                            _ => response(&mut stream,"ERR join-rejected")?,
                        }
                    }
                    "INPUT" if !arg.is_empty() && arg.len() <= 256 => {
                        if shared.input(session,text_intent(arg)).is_ok() {
                            response(&mut stream,"OK queued")?;
                        } else {
                            response(&mut stream,"ERR input-rejected")?;
                        }
                    }
                    "POLL" => {
                        match shared.observe(session) {
                            Ok(snapshots) => {
                                response(&mut stream,&format!("COUNT {}",snapshots.len()))?;
                                for snap in snapshots {
                                    response(&mut stream,&format!(
                                        "SNAPSHOT {} {}",snap.entity_id.0,snap.revision.0,
                                    ))?;
                                }
                                response(&mut stream,"END")?;
                            }
                            Err(_) => response(&mut stream,"ERR observe-rejected")?,
                        }
                    }
                    _ => response(&mut stream,"ERR unknown-command")?,
                }
            }
        }
    }
    if let Some(handle) = ticket {
        if let Ok(mut shared) = universe.lock() { shared.disconnect(handle); }
    }
    Ok(())
}
