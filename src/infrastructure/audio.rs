use reqwest::Client;
use songbird::input::{Input, YoutubeDl};

fn best_audio_args() -> Vec<String> {
    vec!["-f".to_string(), "bestaudio".to_string()]
}

pub struct AudioSource;

impl AudioSource {
    pub fn from_url(http: Client, url: &str) -> Input {
        YoutubeDl::new(http, url.to_string())
            .user_args(best_audio_args())
            .into()
    }

    pub fn from_search(http: Client, query: &str) -> Input {
        YoutubeDl::new_search(http, query.to_string())
            .user_args(best_audio_args())
            .into()
    }
}
