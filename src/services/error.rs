#[derive(Debug, thiserror::Error)]
pub enum MusicError {
    #[error("You must be in a voice channel")]
    NotInVoiceChannel,
    #[error("This command must be used in a server")]
    NotInGuild,
    #[error("No results found for your query")]
    NoResults,
    #[error("The queue is empty")]
    EmptyQueue,
    #[error("Failed to join voice channel: {0}")]
    JoinError(String),
}
