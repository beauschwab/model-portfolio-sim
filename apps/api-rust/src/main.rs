//! `workbench-native api | worker [--exit-when-idle] | admin <command> | contract <kind>`.
use std::sync::atomic::AtomicBool;
use workbench_native::{
    config::Config, contract, http, migrate, objects::Objects, repo::Repo, sql::Db,
};

fn usage() -> ! {
    eprintln!(
        "usage: workbench-native api [--threads N]\n\
         \x20      workbench-native worker [--exit-when-idle]\n\
         \x20      workbench-native admin migrate | init | status | cutover --to rust|legacy [--actor NAME]\n\
         \x20      workbench-native contract openapi|typescript"
    );
    std::process::exit(2)
}

fn repo(config: &Config) -> Result<Repo, String> {
    let db = Db::open(&config.database_url)?;
    migrate::run(&db)?;
    Ok(Repo::new(db, &config.tenant, &config.workspace))
}

fn flag(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1).cloned())
}

fn run(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("contract") => {
            match args.get(1).map(String::as_str) {
                Some("openapi") => println!(
                    "{}",
                    serde_json::to_string_pretty(&contract::openapi()).expect("openapi")
                ),
                Some("typescript") => print!("{}", contract::typescript()),
                _ => usage(),
            }
            Ok(())
        }
        Some("api") => {
            let threads = flag(args, "--threads")
                .and_then(|t| t.parse().ok())
                .unwrap_or(4);
            http::serve(http::App::new(Config::from_env()?)?, threads)
        }
        Some("worker") => {
            let worker = workbench_native::worker::Worker::new(Config::from_env()?)?;
            let stop = AtomicBool::new(false);
            worker.run(args.iter().any(|a| a == "--exit-when-idle"), &stop)
        }
        Some("admin") => {
            let config = Config::from_env()?;
            let actor = flag(args, "--actor").unwrap_or_else(|| "native-admin".into());
            match args.get(1).map(String::as_str) {
                Some("migrate") => {
                    let db = Db::open(&config.database_url)?;
                    let applied = migrate::run(&db)?;
                    println!("{}", serde_json::json!({"applied": applied}));
                }
                Some("init") => {
                    let repo = repo(&config)?;
                    let objects =
                        Objects::open(&config.artifact_url, &config.tenant, &config.workspace)?;
                    let native = repo
                        .initialize(&objects, &actor)
                        .map_err(|r| r.to_string())?;
                    println!(
                        "{}",
                        serde_json::json!({"native_workspace": native, "revision": repo.head().map_err(|r| r.to_string())?})
                    );
                }
                Some("cutover") => {
                    let to = flag(args, "--to").unwrap_or_else(|| usage());
                    let (engine, epoch) = repo(&config)?
                        .cutover(&to, &actor)
                        .map_err(|r| r.to_string())?;
                    println!("{}", serde_json::json!({"engine": engine, "epoch": epoch}));
                }
                Some("status") => {
                    let repo = repo(&config)?;
                    let (engine, epoch) = repo.route().map_err(|r| r.to_string())?;
                    let counts: serde_json::Map<String, serde_json::Value> = [
                        "queued",
                        "running",
                        "native_queued",
                        "native_running",
                        "native_done",
                        "native_error",
                        "native_cancelled",
                    ]
                    .iter()
                    .map(|s| {
                        Ok((
                            s.to_string(),
                            repo.count_status(s).map_err(|r| r.to_string())?.into(),
                        ))
                    })
                    .collect::<Result<_, String>>()?;
                    println!(
                        "{}",
                        serde_json::json!({"engine": engine, "epoch": epoch,
                        "revision": repo.head().map_err(|r| r.to_string())?, "jobs": counts})
                    );
                }
                _ => usage(),
            }
            Ok(())
        }
        _ => usage(),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Err(e) = run(&args) {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
