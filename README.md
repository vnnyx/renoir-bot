# renoir-bot

A Discord music bot written in Rust, supporting YouTube and Spotify playback with a queue system.

## Features

- Play music from YouTube (URLs, video links, playlists) and Spotify (tracks, playlists, albums)
- Search by text query with autocomplete suggestions
- Per-guild queue with now-playing messages and interactive controls
- Pause/resume, skip, seek ±15s, repeat, and stop via button components
- Parallel metadata fetching and background playlist enqueuing
- Inactivity auto-disconnect

## Commands

| Command | Description |
|---------|-------------|
| `/play <query>` | Play a YouTube/Spotify URL or search by text |
| `/next` | Skip to the next track |
| `/skip` | Alias for `/next` |
| `/stop` | Stop playback, clear the queue, and leave the voice channel |
| `/list` | Show the current queue |

The now-playing message also provides inline buttons: Pause/Resume, Skip, Stop, Seek -15s/+15s, and Repeat.

## Tech Stack

- **[poise](https://github.com/serenity-rs/poise)** — slash command framework
- **[serenity](https://github.com/serenity-rs/serenity)** — Discord API client
- **[songbird](https://github.com/serenity-rs/songbird)** — voice/audio engine
- **[rspotify](https://github.com/ramsayleung/rspotify)** — Spotify Web API client
- **[yt-dlp](https://github.com/yt-dlp/yt-dlp)** — audio extraction (runtime dependency)
- **tokio** — async runtime

## Prerequisites

- Rust (see `rust-toolchain.toml`)
- `yt-dlp` installed and on `PATH`
- `opus` library (runtime)
- A Discord bot token
- A Spotify app (client ID + secret)
- A YouTube Data API v3 key

## Configuration

Create a `.env` file in the project root:

```env
DISCORD_TOKEN=your_discord_bot_token
SPOTIFY_CLIENT_ID=your_spotify_client_id
SPOTIFY_CLIENT_SECRET=your_spotify_client_secret
YOUTUBE_API_KEY=your_youtube_api_key
```

## Running Locally

```bash
# Install system dependencies (Debian/Ubuntu)
apt-get install libopus-dev

# Install yt-dlp
pip install yt-dlp

cargo run --release
```

## Docker

```bash
docker build -t renoir-bot .
docker run --env-file .env renoir-bot
```

The Docker image uses a multi-stage build: a `rust:1.88.0-slim-bookworm` builder stage and a `debian:bookworm-slim` runtime stage with `yt-dlp` pre-installed.

## Project Structure

```
src/
├── main.rs                  # Bot setup, shared state, framework registration
├── config.rs                # Environment variable loading
├── domain/
│   ├── track.rs             # Track and TrackSource types
│   └── queue.rs             # MusicQueue domain model
├── infrastructure/
│   ├── audio.rs             # AudioSource (songbird YoutubeDl wrapper)
│   ├── spotify.rs           # SpotifyClient (rspotify)
│   ├── youtube.rs           # YouTubeClient (YouTube Data API)
│   └── inactivity.rs        # Inactivity monitor task
├── services/
│   ├── music_service.rs     # Parallel search, URL parsing, query building
│   ├── queue_service.rs     # Per-guild queue management
│   ├── cleanup.rs           # Guild state teardown
│   └── error.rs             # MusicError types
└── commands/
    ├── play.rs              # /play, voice join, enqueue logic, event handlers
    ├── stop.rs              # /stop
    ├── next.rs              # /next
    ├── skip.rs              # /skip
    ├── list.rs              # /list
    └── now_playing.rs       # Now-playing button interactions
```
