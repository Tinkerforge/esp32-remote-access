use backend::x25519::{PublicKey, StaticSecret};
use crypto::generate_hash;
use http::{add_charger, get_login_salt, get_secret, login, management_discovery};

mod http;
mod crypto;
mod socket;

const ID: i32 = 12345;

struct State {
    local_management_secret: StaticSecret,
    server_management_public: PublicKey,
    password: String,
    remote_keys: Vec<(StaticSecret, PublicKey)>
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    simplelog::CombinedLogger::init(vec![simplelog::TermLogger::new(
        simplelog::LevelFilter::Debug,
        simplelog::Config::default(),
        simplelog::TerminalMode::Mixed,
        simplelog::ColorChoice::Auto,
    )])
    .unwrap();
    unsafe {
        libsodium_sys::sodium_init();
    }

    dotenv::dotenv().ok();

    let args: Vec<String> = std::env::args().collect();
    let url = &args[1];
    let email = std::env::var("EMAIL").expect("Need a Username");
    let password = std::env::var("PASSWORD").expect("Need a Password");

    log::info!("Starting");

    log::info!("Getting Login salt");
    let login_salt = get_login_salt(&email, url).await?;
    let login_key = generate_hash(password.as_bytes(), &login_salt, None)?;

    log::info!("Logging in");
    let access_token = login(email, login_key, url).await?;
    log::info!("Getting secret");
    let secret = get_secret(&access_token, &password, url).await?;
    log::info!("Adding charger");
    let state = add_charger(&access_token, &secret, url).await?;
    log::info!("Sending management discovery");
    management_discovery(&state, url).await?;


    Ok(())
}
