use reqwest::Client;
use songbird::input::{Input, YoutubeDl};

pub struct AudioSource;

impl AudioSource {
    pub fn from_url(http: Client, url: &str) -> Input {
        YoutubeDl::new(http, url.to_string()).into()
    }

    pub fn from_search(http: Client, query: &str) -> Input {
        YoutubeDl::new_search(http, query.to_string()).into()
    }
}
