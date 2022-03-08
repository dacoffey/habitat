use crate::{error::{Error,
                    Result},
            manager::file_watcher::{create_file_watcher,
                                    Callbacks}};
use habitat_butterfly::member::Member;
use habitat_common::{liveliness_checker,
                     outputln,
                     types::GossipListenAddr};
use std::{fs::File,
          io::{BufRead,
               BufReader},
          net::{SocketAddr,
                ToSocketAddrs},
          path::{Path,
                 PathBuf},
          sync::{atomic::{AtomicBool,
                          Ordering},
                 Arc},
          thread::Builder as ThreadBuilder};

static LOGKEY: &str = "PW";

pub struct PeerCallbacks {
    have_events: Arc<AtomicBool>,
}

impl Callbacks for PeerCallbacks {
    fn file_appeared(&mut self, _: &Path) { self.have_events.store(true, Ordering::Relaxed); }

    fn file_modified(&mut self, _: &Path) { self.have_events.store(true, Ordering::Relaxed) }

    fn file_disappeared(&mut self, _: &Path) { self.have_events.store(true, Ordering::Relaxed) }
}

pub struct PeerWatcher {
    path:        PathBuf,
    have_events: Arc<AtomicBool>,
}

impl PeerWatcher {
    pub fn run<P>(path: P) -> Result<Self>
        where P: Into<PathBuf>
    {
        let path = path.into();
        let have_events = Self::setup_watcher(path.clone())?;

        Ok(PeerWatcher { path, have_events })
    }

    fn setup_watcher(path: PathBuf) -> Result<Arc<AtomicBool>> {
        let have_events = Arc::new(AtomicBool::new(false));
        let have_events_for_thread = Arc::clone(&have_events);

        ThreadBuilder::new().name(format!("peer-watcher-[{}]", path.display()))
                            .spawn(move || -> liveliness_checker::ThreadUnregistered {
                                // debug!("PeerWatcher({}) thread starting", abs_path.display());
                                loop {
                                    let checked_thread = liveliness_checker::mark_thread_alive();
                                    let have_events_for_loop = Arc::clone(&have_events_for_thread);
                                    if Self::file_watcher_loop_body(&path, have_events_for_loop) {
                                        break checked_thread.unregister(Ok(()));
                                    }
                                }
                            })?;
        Ok(have_events)
    }

    fn file_watcher_loop_body(path: &Path, have_events: Arc<AtomicBool>) -> bool {
        let callbacks = PeerCallbacks { have_events };
        let mut file_watcher = match create_file_watcher(&path, callbacks, true) {
            Ok(watcher) => watcher,
            Err(e) => {
                match e {
                    Error::NotifyError(err) => {
                        outputln!("PeerWatcher({}) failed to start watching the directories \
                                   ({}), {}",
                                  path.display(),
                                  err,
                                  "will try again");
                        return false;
                    }
                    _ => {
                        outputln!("PeerWatcher({}) could not create file watcher, ending thread \
                                   ({})",
                                  path.display(),
                                  e);
                        return true;
                    }
                }
            }
        };
        if let Err(err) = file_watcher.run() {
            outputln!("PeerWatcher({}) error during watching ({}), restarting",
                      path.display(),
                      err);
        }
        false
    }

    pub fn has_fs_events(&self) -> bool { self.have_events.load(Ordering::Relaxed) }

    pub fn get_members(&self) -> Result<Vec<Member>> {
        if !self.path.is_file() {
            self.have_events.store(false, Ordering::Relaxed);
            return Ok(Vec::new());
        }
        let file = File::open(&self.path).map_err(Error::Io)?;
        let reader = BufReader::new(file);
        let mut members: Vec<Member> = Vec::new();
        for line in reader.lines().flatten() {
            let peer_addr = if line.find(':').is_some() {
                line
            } else {
                format!("{}:{}", line, GossipListenAddr::DEFAULT_PORT)
            };
            let addrs: Vec<SocketAddr> = match peer_addr.to_socket_addrs() {
                Ok(addrs) => addrs.collect(),
                Err(e) => {
                    outputln!("Failed to resolve peer: {}", peer_addr);
                    return Err(Error::NameLookup(e));
                }
            };
            let addr: SocketAddr = addrs[0];
            let member = Member { address: format!("{}", addr.ip()),
                                  swim_port: addr.port(),
                                  gossip_port: addr.port(),
                                  ..Default::default() };
            members.push(member);
        }
        self.have_events.store(false, Ordering::Relaxed);
        Ok(members)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use habitat_butterfly::member::Member;
    use std::{env,
              fs::{File,
                   OpenOptions},
              io::Write};
    use tempfile::TempDir;

    #[test]
    fn no_file() {
        let tmpdir = TempDir::new().unwrap();
        let path = tmpdir.path().join("no_such_file");
        let watcher = PeerWatcher::run(path).unwrap();

        assert!(!watcher.has_fs_events());
        assert_eq!(watcher.get_members().unwrap(), vec![]);
    }

    #[test]
    fn empty_file() {
        let tmpdir = TempDir::new().unwrap();
        let path = tmpdir.path().join("empty_file");
        File::create(&path).unwrap();
        let watcher = PeerWatcher::run(path).unwrap();

        assert_eq!(watcher.get_members().unwrap(), vec![]);
    }

    #[test]
    fn with_file() {
        let tmpdir = TempDir::new().unwrap();
        let path = tmpdir.path().join("some_file");
        let mut file = OpenOptions::new().append(true)
                                         .create_new(true)
                                         .open(path.clone())
                                         .unwrap();
        let watcher = PeerWatcher::run(path).unwrap();
        writeln!(file, "1.2.3.4:5").unwrap();
        writeln!(file, "4.3.2.1").unwrap();
        let member1 = Member { id: String::new(),
                               address: String::from("1.2.3.4"),
                               swim_port: 5,
                               gossip_port: 5,
                               ..Default::default() };
        let member2 = Member { id: String::new(),
                               address: String::from("4.3.2.1"),
                               swim_port: GossipListenAddr::DEFAULT_PORT,
                               gossip_port: GossipListenAddr::DEFAULT_PORT,
                               ..Default::default() };
        let expected_members = vec![member1, member2];
        let mut members = watcher.get_members().unwrap();
        for mut member in &mut members {
            member.id = String::new();
        }
        assert_eq!(expected_members, members);
    }

    #[test]
    fn with_file_using_poll_watcher() {
        env::set_var("HAB_STUDIO_HOST_ARCH", "aarch64-darwin");
        let tmpdir = TempDir::new().unwrap();
        let path = tmpdir.path().join("some_other_file");
        let mut file = OpenOptions::new().append(true)
                                         .create_new(true)
                                         .open(path.clone())
                                         .unwrap();
        let watcher = PeerWatcher::run(path).unwrap();
        writeln!(file, "1.2.3.5:5").unwrap();
        writeln!(file, "5.4.3.2").unwrap();
        let member1 = Member { id: String::new(),
                               address: String::from("1.2.3.5"),
                               swim_port: 5,
                               gossip_port: 5,
                               ..Default::default() };
        let member2 = Member { id: String::new(),
                               address: String::from("5.4.3.2"),
                               swim_port: GossipListenAddr::DEFAULT_PORT,
                               gossip_port: GossipListenAddr::DEFAULT_PORT,
                               ..Default::default() };

        let expected_members = vec![member1, member2];
        let mut members = watcher.get_members().unwrap();
        for mut member in &mut members {
            member.id = String::new();
        }
        assert_eq!(expected_members, members);
        env::remove_var("HAB_STUDIO_HOST_ARCH");
    }
}
