use {ErrorKind, Result};

/// Stream identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StreamId(u8);
impl StreamId {
    /// Minimum value of the identifiers for audio streams.
    pub const AUDIO_MIN: u8 = 0xC0;

    /// Maximum value of the identifiers for audio streams.
    pub const AUDIO_MAX: u8 = 0xDF;

    /// Minimum value of the identifiers for video streams.
    pub const VIDEO_MIN: u8 = 0xE0;

    /// Maximum value of the identifiers for video streams.
    pub const VIDEO_MAX: u8 = 0xEF;

    //Synchronous KLV identifier
    pub const KLV_SYNC: u8 = 0xBD;

    //Synchronous KLV identifier
    pub const KLV_ASYNC: u8 = 0xFC;


    /// Makes a new `StreamId` instance.
    pub fn new(id: u8) -> Self {
        StreamId(id)
    }

    /// Makes a new `StreamId` instance for audio stream.
    ///
    /// # Errors
    ///
    /// If `id` is not between `AUDIO_MIN` and `AUDIO_MAX`, it will return an `ErrorKind::InvalidInput` error.
    pub fn new_audio(id: u8) -> Result<Self> {
        track_assert!(
            Self::AUDIO_MIN <= id && id <= Self::AUDIO_MAX,
            ErrorKind::InvalidInput,
            "Not an audio ID: {}",
            id
        );
        Ok(StreamId(id))
    }

    /// Makes a new `StreamId` instance for video stream.
    ///
    /// # Errors
    ///
    /// If `id` is not between `VIDEO_MIN` and `VIDEO_MAX`, it will return an `ErrorKind::InvalidInput` error.
    pub fn new_video(id: u8) -> Result<Self> {
        track_assert!(
            Self::VIDEO_MIN <= id && id <= Self::VIDEO_MAX,
            ErrorKind::InvalidInput,
            "Not a video ID: {}",
            id
        );
        Ok(StreamId(id))
    }

    /// Returns the value of the identifier.
    pub fn as_u8(&self) -> u8 {
        self.0
    }

    /// Returns `true` if it is an audio identifier, otherwise `false`.
    pub fn is_audio(&self) -> bool {
        0xC0 <= self.0 && self.0 <= 0xDF
    }

    /// Returns `true` if it is a video identifier, otherwise `false`.
    pub fn is_video(&self) -> bool {
        0xE0 <= self.0 && self.0 <= 0xEF
    }

    /// Returns `true` if this contains a klv identifier, otherwise `false`.
    pub fn is_klv(&self) -> bool {
        return Self::is_async_klv(&self) || Self::is_sync_klv(&self)
    }

    //Returns true if this contains a synchronous klv identifier, otherwise `false`.
    pub fn is_sync_klv(&self) -> bool {
        self.0 == Self::KLV_SYNC
    }

    //Returns true if this contains an asynchronous klv identifier, otherwise `false`.
    pub fn is_async_klv(&self) -> bool {
        self.0 == Self::KLV_ASYNC
    }
}
