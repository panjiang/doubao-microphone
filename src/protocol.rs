use anyhow::{Result, bail, ensure};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const VERSION: u32 = 1;
pub const FRAME_HEADER: usize = 24;
pub const MAX_MESSAGE: usize = FRAME_HEADER + 3840;

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ClientMessage {
    Hello {
        version: u32,
        token: String,
        sample_rate: u32,
        channels: u8,
    },
    Start {
        session_id: Uuid,
    },
    End {
        session_id: Uuid,
        last_seq: Option<u32>,
    },
    Cancel {
        session_id: Uuid,
    },
    Heartbeat,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    Hello {
        version: u32,
        mock: bool,
    },
    Ready {
        session_id: Uuid,
    },
    Ended {
        session_id: Uuid,
        complete: bool,
        reason: String,
    },
    Heartbeat,
    Error {
        code: String,
        message: String,
    },
}

#[derive(Debug)]
pub struct AudioFrame {
    pub session_id: Uuid,
    pub sequence: u32,
    pub samples: Vec<f32>,
}

impl AudioFrame {
    pub fn decode(data: &[u8], sample_rate: u32) -> Result<Self> {
        ensure!(
            data.len() >= FRAME_HEADER + 2,
            "empty or truncated audio frame"
        );
        ensure!(&data[..4] == b"PPT1", "invalid audio magic");
        ensure!(
            data.len() <= MAX_MESSAGE && (data.len() - FRAME_HEADER).is_multiple_of(2),
            "invalid audio length"
        );
        let count = (data.len() - FRAME_HEADER) / 2;
        ensure!(
            count <= (sample_rate / 50) as usize,
            "audio frame exceeds 20ms"
        );
        let session_id = Uuid::from_slice(&data[4..20])?;
        let sequence = u32::from_le_bytes(data[20..24].try_into()?);
        let samples = data[24..]
            .chunks_exact(2)
            .map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0)
            .collect();
        Ok(Self {
            session_id,
            sequence,
            samples,
        })
    }
}

pub fn validate_format(rate: u32, channels: u8) -> Result<()> {
    if !(8_000..=96_000).contains(&rate) || !rate.is_multiple_of(50) || channels != 1 {
        bail!("only mono PCM16 at 8000..96000 Hz in 20ms frames is accepted");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn framing_checks_identity_bounds_and_signed_pcm() {
        let id = Uuid::new_v4();
        let mut b = b"PPT1".to_vec();
        b.extend(id.as_bytes());
        b.extend(7u32.to_le_bytes());
        b.extend(i16::MIN.to_le_bytes());
        let f = AudioFrame::decode(&b, 48000).unwrap();
        assert_eq!(f.session_id, id);
        assert_eq!(f.sequence, 7);
        assert_eq!(f.samples, [-1.0]);
        assert!(AudioFrame::decode(&b[..24], 48000).is_err());
        b.extend(vec![0; 4000]);
        assert!(AudioFrame::decode(&b, 48000).is_err());
        assert!(validate_format(48000, 2).is_err());
    }
}
