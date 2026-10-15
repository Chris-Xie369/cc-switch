//! One-shot, explicit migration using the same exporter as the desktop app.
//! Usage: cargo run --example dsh-managed-sync -- <database> [--apply]
//! No credential contents or provider configuration are printed.
use cc_switch_lib::dsh_config;
use rusqlite::{Connection, OpenFlags};
use serde_json::Value;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let db_path = args.get(1).ok_or("Supply database path; add --apply to migrate")?;
    let apply = args.get(2).map(String::as_str) == Some("--apply");
    let db = Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let mut query = db.prepare("SELECT id,name,settings_config FROM providers WHERE app_type='deepseek-harness' ORDER BY id")?;
    let cards = query.query_map([], |row| Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,String>(2)?)))?.collect::<Result<Vec<_>,_>>()?;
    let cards: Vec<_> = cards.into_iter().map(|(id,name,raw)| serde_json::from_str::<Value>(&raw).map(|config|(id,name,config))).collect::<Result<_,_>>()?;
    if !apply {
        for (id,name,config) in &cards { println!("{}", serde_json::json!({"provider":name,"route":dsh_config::sanitize_route(id),"models":config["models"].as_object().map(|m|m.len())})); }
        return Ok(());
    }
    let root = dsh_config::get_dsh_dir()?;
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_secs();
    let backup = root.join("backups").join(format!("model-capability-{stamp}"));
    std::fs::create_dir_all(&backup)?;
    let mut files = vec![root.join(".credentials.yaml"),root.join("cc-switch-managed.json")];
    for entry in std::fs::read_dir(root.join("profiles"))? {
        let dir = entry?.path();
        if dir.is_dir() && dir.join("cordis.yml").exists() { files.push(dir.join("cordis.patch.yml")); }
    }
    let mut saved: Vec<(PathBuf, Option<PathBuf>)> = Vec::new();
    for (index,file) in files.into_iter().enumerate() {
        let original = if file.exists() { let target=backup.join(format!("{index}.bak")); std::fs::copy(&file,&target)?; Some(target) } else { None };
        saved.push((file, original));
    }
    let outcome = cards.iter().try_for_each(|(id,name,config)| dsh_config::write_managed_provider_card(id,name,config));
    if let Err(error) = outcome {
        for (file,original) in &saved {
            if let Some(original) = original { std::fs::copy(original,file)?; }
            else if file.exists() { std::fs::remove_file(file)?; }
        }
        return Err(error.into());
    }
    // The manifest contains filenames only; secret backups stay outside the repository.
    std::fs::write(backup.join("files.json"),serde_json::to_vec_pretty(&saved)?)?;
    println!("{}",serde_json::json!({"applied":cards.len(),"backup":backup,"historicalRoutesPreserved":true}));
    Ok(())
}
