//! Bounded actor registry and owner-thread dispatch for retained native sessions.
use super::session::Session;
use crate::coefficients::{
    aged as aged_library, commercial as commercial_library, monthly as library,
    products as product_library,
};
use crate::optimization::highs as solver;
use crate::workflows::owned;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc, Mutex, OnceLock,
    },
};
type Message = (
    Value,
    mpsc::SyncSender<Result<Value, String>>,
    Option<portfolio_compute_control::Control>,
);
enum Actor {
    Legacy(Box<Session>, Option<Box<owned::Owner>>),
    Aged(Box<aged_library::Session>),
    Product(Box<product_library::Session>),
    Commercial(Box<commercial_library::Session>),
}
impl Actor {
    fn call(&mut self, msg: Value) -> Result<Value, String> {
        match self {
            Self::Product(session) => session.call(msg),
            Self::Commercial(session) => session.call(msg),
            Self::Aged(session) => session.call(msg),
            Self::Legacy(session, owner) => {
                if let Some(owner) = owner.as_mut() {
                    match msg["op"].as_str() {
                        Some("update_owned") => owner.update(session, msg),
                        Some("publish") => {
                            let result = session.call(msg)?;
                            owner.publish();
                            Ok(result)
                        }
                        Some("abort") => {
                            let result = session.call(msg)?;
                            owner.abort();
                            Ok(result)
                        }
                        Some("eval" | "status") => session.call(msg),
                        _ => Err("owned session requires native orchestration".into()),
                    }
                } else {
                    session.call(msg)
                }
            }
        }
    }
}
#[derive(Clone)]
pub(crate) struct ActorHandle {
    pub(crate) sender: mpsc::SyncSender<Message>,
    pub(crate) commercial: bool,
}
pub(crate) static REGISTRY: OnceLock<Mutex<HashMap<u64, ActorHandle>>> = OnceLock::new();
pub(crate) fn commercial_handle(handle: u64) -> Result<bool, String> {
    let Some(registry) = REGISTRY.get() else {
        return Ok(false);
    };
    Ok(registry
        .lock()
        .map_err(|_| "registry unavailable")?
        .get(&handle)
        .is_some_and(|actor| actor.commercial))
}
static NEXT: AtomicU64 = AtomicU64::new(1);
pub fn dispatch(v: Value) -> Result<Value, String> {
    let registry = REGISTRY.get_or_init(|| Mutex::new(HashMap::new()));
    let op = v["op"].as_str().ok_or("missing operation")?;
    if op == "run_owned" {
        return owned::run(v);
    }
    if op == "optimize_library" {
        return library::optimize(&v);
    }
    if op == "optimize_aged" {
        return aged_library::optimize(&v);
    }
    if op == "optimize_product" {
        return product_library::optimize(&v);
    }
    if op == "optimize_commercial" {
        return commercial_library::optimize(&v);
    }
    if op == "linear_program" {
        return solver::linear_program(&v);
    }
    if op == "create"
        || op == "create_owned"
        || op == "create_aged"
        || op == "create_product"
        || op == "create_commercial"
    {
        let mut slots = registry.lock().map_err(|_| "registry unavailable")?;
        if slots.len() >= 8 {
            return Err("native session capacity reached (8); close a session".into());
        }
        let handle = NEXT.fetch_add(1, Ordering::Relaxed);
        if handle == 0 {
            return Err("native session handle exhausted".into());
        }
        let (tx, rx) = mpsc::sync_channel::<Message>(8);
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        // Reserve capacity atomically, then release the registry before pricing.
        // Opening a book must not block evaluation/close on unrelated sessions.
        slots.insert(
            handle,
            ActorHandle {
                sender: tx,
                commercial: op == "create_commercial",
            },
        );
        drop(slots);
        // A creation deadline belongs to this call, never to the retained actor.
        let creation_control = if op == "create_commercial" {
            portfolio_compute_control::active()
        } else {
            None
        };
        let spawned = std::thread::Builder::new()
            .name(format!("decision-{handle}"))
            .spawn(move || {
                let mut v = v;
                let created = if v["op"] == "create_commercial" {
                    portfolio_compute_control::scope(creation_control, || {
                        commercial_library::Session::new(&v)
                    })
                    .map(|(session, snapshot)| (Actor::Commercial(Box::new(session)), snapshot))
                } else if v["op"] == "create_product" {
                    product_library::Session::new(&v)
                        .map(|(session, snapshot)| (Actor::Product(Box::new(session)), snapshot))
                } else if v["op"] == "create_aged" {
                    aged_library::Session::new(&v)
                        .map(|(session, snapshot)| (Actor::Aged(Box::new(session)), snapshot))
                } else if v["op"] == "create_owned" {
                    owned::Owner::new(&mut v).map(|(owner, session, snapshot)| {
                        (
                            Actor::Legacy(Box::new(session), Some(Box::new(owner))),
                            snapshot,
                        )
                    })
                } else {
                    Session::new(&v)
                        .map(|session| (Actor::Legacy(Box::new(session), None), json!({})))
                };
                let mut actor = match created {
                    Ok((actor, snapshot)) => {
                        let _ = ready_tx.send(Ok(snapshot));
                        actor
                    }
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                        return;
                    }
                };
                while let Ok((msg, reply, control)) = rx.recv() {
                    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        if matches!(actor, Actor::Commercial(_)) {
                            portfolio_compute_control::scope(control, || actor.call(msg))
                        } else {
                            actor.call(msg)
                        }
                    })) {
                        Ok(r) => {
                            let _ = reply.send(r);
                        }
                        Err(_) => {
                            let _ =
                                reply.send(Err("native session panicked and was closed".into()));
                            break;
                        }
                    }
                }
                if let Ok(mut slots) = registry.lock() {
                    slots.remove(&handle);
                }
            })
            .map_err(|e| e.to_string());
        let ready =
            spawned.and_then(|_| ready_rx.recv().map_err(|e| e.to_string()).and_then(|v| v));
        let snapshot = match ready {
            Ok(snapshot) => snapshot,
            Err(error) => {
                registry
                    .lock()
                    .map_err(|_| "registry unavailable")?
                    .remove(&handle);
                return Err(error);
            }
        };
        return Ok(json!({"handle":handle,"version":0,"snapshot":snapshot}));
    }
    let handle = v["handle"].as_u64().ok_or("missing handle")?;
    if op == "close" {
        registry
            .lock()
            .map_err(|_| "registry unavailable")?
            .remove(&handle)
            .ok_or("unknown session")?;
        return Ok(json!({"closed":true}));
    }
    let tx = registry
        .lock()
        .map_err(|_| "registry unavailable")?
        .get(&handle)
        .cloned()
        .ok_or("unknown session")?;
    let (reply, rx) = mpsc::sync_channel(1);
    tx.sender
        .try_send((v, reply, portfolio_compute_control::active()))
        .map_err(|e| e.to_string())?;
    rx.recv().map_err(|e| e.to_string())?
}
