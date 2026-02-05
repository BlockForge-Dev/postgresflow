// Clone: lets you safely duplicate the config

#[derive(Clone, Debug)]

// Config is a central place for runtime configuration

// It loads values from environment variables

// It gives you a typed, validated struct instead of raw strings everywhere
pub struct Config {
    pub database_url: String,
    pub worker_id: String,
}

impl Config {
    //Result<String, VarError>
    pub fn from_env() -> anyhow::Result<Self> {
        dotenvy::dotenv().ok();

        let database_url = std::env::var("DATABASE_URL")
            .map_err(|_| anyhow::anyhow!("DATABASE_URL is missing"))?;
        //.map_err(...) converts that error into an anyhow::Error
        //std::env::var returns Result<String, VarError>

        let worker_id = std::env::var("WORKER_ID").unwrap_or_else(|_| "worker-1".to_string());

        Ok(Self {
            database_url,
            worker_id,
        })
    }

    //   Construct a Config

    // Wrap it in Ok

    // Return it to the caller
}
