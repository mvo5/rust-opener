use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, ErrorKind, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

use serde::Deserialize;

use rand::distributions::Alphanumeric;
use rand::{thread_rng, Rng};

use sjm::SignedJsonMessage;

fn gen_nonce() -> String {
    let mut rng = thread_rng();

    (&mut rng)
        .sample_iter(Alphanumeric)
        .take(32)
        .map(char::from)
        .collect()
}

// XXX: actually implement a open for esp32 rust :)
fn toggle_pin() -> () {
    println!("toggle pin");
}

fn send_with_hmac(
    mut stream: &TcpStream,
    hmac_key: &str,
    nonce: &str,
    msg: HashMap<String, String>,
) -> std::io::Result<()> {
    let mut sjm = SignedJsonMessage::new(hmac_key, &nonce);
    // XXX: switch sjm to use serde_json::Map instead of HashMap
    sjm.payload = msg;
    // XXX: use anyhow::Error or something here
    let mut s = sjm
        .to_string()
        .map_err(|e| std::io::Error::new(ErrorKind::Other, e))?;
    s.push('\n');
    stream.write(&s.as_bytes())?;

    Ok(())
}

fn recv_with_hmac(
    stream: &TcpStream,
    hmac_key: &str,
    nonce: &str,
) -> std::io::Result<HashMap<String, String>> {
    // XXX: add recv_with_hmac()
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    // XXX: discard empty lines here?
    reader.read_line(&mut line)?;
    let line = line.trim();
    let cmd = SignedJsonMessage::from_string(&line, hmac_key, &nonce)
        .map_err(|e| std::io::Error::new(ErrorKind::Other, e))?;

    Ok(cmd.payload)
}

fn handle_client_connection(
    hmac_key: &str,
    stream: Result<TcpStream, std::io::Error>,
) -> std::io::Result<()> {
    let stream = stream?;

    println!("remote client: {}", stream.peer_addr()?);
    // set timeouts, the microcontroller is single threaded
    stream
        .set_read_timeout(Some(Duration::new(5, 0)))
        .expect("internal error: cannot set read timeout");
    stream
        .set_write_timeout(Some(Duration::new(5, 0)))
        .expect("internal error: cannot set write timeout");

    // gen session nonce
    let nonce = gen_nonce();

    // send header/nonce/api
    let payload = HashMap::from([
        // XXX: the python implementation uses type "int" here for "1"
        ("version".to_string(), "1".to_string()),
        ("api".to_string(), "opener".to_string()),
    ]);
    send_with_hmac(&stream, &hmac_key, &nonce, payload)?;

    // expect command from client
    let cmd = recv_with_hmac(&stream, &hmac_key, &nonce)?;
    match cmd.get("cmd").map_or("", String::as_ref) {
        "open" => toggle_pin(),
        unknown => Err(std::io::Error::new(
            ErrorKind::Other,
            format!("unknown command {unknown}"),
        ))?,
    }

    // send ok
    let payload = HashMap::from([("status".to_string(), "ok".to_string())]);
    send_with_hmac(&stream, &hmac_key, &nonce, payload)?;

    Ok(())
}

fn wait_for_commands(
    hmac_key: &str,
    hostname: &str,
    port: u16,
    opener_pin: u8,
) -> std::io::Result<()> {
    println!("Cfg: {hmac_key} {hostname} {port} {opener_pin}");

    let listener = TcpListener::bind(format!("0.0.0.0:{port}"))?;

    // XXX: add watchdog and feed
    for stream in listener.incoming() {
        match handle_client_connection(hmac_key, stream) {
            Ok(_) => (),
            Err(err) => println!("cannot handle client: {err}"),
        }
    }

    Ok(())
}

const fn default_port() -> u16 {
    8877
}

#[derive(Deserialize, Debug)]
struct Config {
    #[serde(rename = "hmac-key")]
    hmac_key: String,
    hostname: String,
    #[serde(rename = "opener-gpio-pin")]
    opener_pin: u8,
    #[serde(default = "default_port")]
    port: u16,
    //ssid: String,
    //#[serde(default,rename = "telegram-bot-token")]
    //telegram_bot_token: String,
    //#[serde(rename = "telegram-chat-id")]
    //telegram_chat_id: String,
}

fn read_config() -> Result<Config, std::io::Error> {
    let file = File::open("config.json")?;
    let reader = BufReader::new(file);
    let cfg = serde_json::from_reader(reader)?;

    Ok(cfg)
}

// missing
// - TESTS
// - telegram client
// - actual gpio support :)
// - logging
// - watchdog
// - some sort of crash handling like mupy-open?
// - error handling (anyhow::?)
// - code cleanups

fn main() {
    let cfg = read_config().expect("cannot read config");

    wait_for_commands(&cfg.hmac_key, &cfg.hostname, cfg.port, cfg.opener_pin)
        .expect("wait_for_commands failed");
}
