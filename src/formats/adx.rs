//! CRI ADX audio, standard encoding with 18 byte blocks.

use anyhow::{bail, Result};

/// Decoded audio. Samples of several channels take turns.
#[derive(Debug, Clone, PartialEq)]
pub struct Audio {
    pub channels: u16,
    pub sample_rate: u32,
    pub samples: Vec<i16>,
}

const SAMPLES_PER_BLOCK: usize = 32;

/// The prediction coefficients from the header's high pass frequency, in 1/4096.
fn coefficients(cutoff: f64, rate: f64) -> (i32, i32) {
    let a = std::f64::consts::SQRT_2 - (2.0 * std::f64::consts::PI * cutoff / rate).cos();
    let b = std::f64::consts::SQRT_2 - 1.0;
    let c = (a - ((a + b) * (a - b)).sqrt()) / b;
    ((c * 8192.0).round() as i32, (-(c * c) * 4096.0).round() as i32)
}

pub fn decode(data: &[u8]) -> Result<Audio> {
    if data.len() < 0x14 || data[0] != 0x80 || data[1] != 0 {
        bail!("not an ADX file");
    }
    let be16 = |at: usize| u16::from_be_bytes([data[at], data[at + 1]]) as usize;
    let be32 = |at: usize| u32::from_be_bytes([data[at], data[at + 1], data[at + 2], data[at + 3]]);
    let start = be16(2) + 4;
    let (encoding, block_size, depth, channels) = (data[4], data[5] as usize, data[6], data[7] as usize);
    if encoding != 3 || block_size != 18 || depth != 4 || channels == 0 {
        bail!("ADX encoding {encoding}, block {block_size}, depth {depth}, {channels} channels is not handled");
    }
    let sample_rate = be32(8);
    if sample_rate == 0 {
        bail!("ADX with no sample rate");
    }
    let total = be32(12) as usize;
    let (c1, c2) = coefficients(be16(0x10) as f64, sample_rate as f64);

    let mut history = vec![(0i32, 0i32); channels];
    let mut samples = Vec::with_capacity(total * channels);
    let frame = block_size * channels;
    let mut produced = 0;
    for frame_data in data.get(start..).unwrap_or(&[]).chunks_exact(frame) {
        if produced >= total {
            break;
        }
        let count = SAMPLES_PER_BLOCK.min(total - produced);
        let mut output = vec![0i16; count * channels];
        let mut ended = false;
        for (channel, block) in frame_data.chunks_exact(block_size).enumerate() {
            let head = u16::from_be_bytes([block[0], block[1]]);
            // The high bit marks the end of the stream.
            if head & 0x8000 != 0 {
                ended = true;
                break;
            }
            let scale = head as i32 + 1;
            let (mut h1, mut h2) = history[channel];
            for i in 0..count {
                let byte = block[2 + i / 2];
                let nibble = if i % 2 == 0 { byte >> 4 } else { byte & 0xF } as i32;
                let signed = if nibble >= 8 { nibble - 16 } else { nibble };
                let value = (signed * scale + ((c1 * h1 + c2 * h2) >> 12)).clamp(-32768, 32767);
                output[i * channels + channel] = value as i16;
                h2 = h1;
                h1 = value;
            }
            history[channel] = (h1, h2);
        }
        if ended {
            break;
        }
        samples.extend_from_slice(&output);
        produced += count;
    }
    Ok(Audio { channels: channels as u16, sample_rate, samples })
}

/// A plain 16 bit WAV file.
pub fn to_wav(audio: &Audio) -> Vec<u8> {
    let bytes = (audio.samples.len() * 2) as u32;
    let align = audio.channels * 2;
    let mut out = Vec::with_capacity(44 + bytes as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + bytes).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&audio.channels.to_le_bytes());
    out.extend_from_slice(&audio.sample_rate.to_le_bytes());
    out.extend_from_slice(&(audio.sample_rate * align as u32).to_le_bytes());
    out.extend_from_slice(&align.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&bytes.to_le_bytes());
    for sample in &audio.samples {
        out.extend_from_slice(&sample.to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clip(channels: u8, total: u32, blocks: &[[u8; 18]]) -> Vec<u8> {
        let mut data = vec![0x80, 0, 0, 0x1C, 3, 18, 4, channels];
        data.extend_from_slice(&22050u32.to_be_bytes());
        data.extend_from_slice(&total.to_be_bytes());
        data.extend_from_slice(&500u16.to_be_bytes());
        data.resize(0x20, 0);
        for block in blocks {
            data.extend_from_slice(block);
        }
        data
    }

    /// Writes each clip of a voice archive as ADX and decoded samples, to compare with
    /// another decoder. DOA2U_VOICES is the archive and ADX_OUT an empty folder.
    #[test]
    #[ignore]
    fn export_clips_for_comparison() {
        let archive = crate::formats::AfsArchive::from_file(std::env::var("DOA2U_VOICES").expect("DOA2U_VOICES")).unwrap();
        let out = std::env::var("ADX_OUT").expect("ADX_OUT");
        for (i, entry) in archive.entries().iter().enumerate() {
            let Some(data) = archive.get_by_index(i) else { continue };
            if data.first() != Some(&0x80) {
                continue;
            }
            let audio = decode(data).unwrap();
            let bytes: Vec<u8> = audio.samples.iter().flat_map(|s| s.to_le_bytes()).collect();
            std::fs::write(format!("{out}/{i}.adx"), data).unwrap();
            std::fs::write(format!("{out}/{i}.raw"), bytes).unwrap();
            let _ = &entry.name;
        }
    }

    #[test]
    fn nibbles_are_signed_and_scaled_and_predicted() {
        let mut block = [0u8; 18];
        block[1] = 9; // Scale 10.
        block[2] = 0x1F; // 1, then -1.
        let audio = decode(&clip(1, 2, &[block])).unwrap();
        let (c1, _) = coefficients(500.0, 22050.0);
        assert_eq!(audio.samples[0], 10);
        assert_eq!(audio.samples[1] as i32, -10 + ((c1 * 10) >> 12));
    }

    #[test]
    fn stereo_takes_turns_and_the_end_mark_stops() {
        let mut left = [0u8; 18];
        left[1] = 1;
        left[2] = 0x10;
        let right = [0u8; 18];
        let mut end = [0u8; 18];
        end[0] = 0x80;
        let data = clip(2, 64, &[left, right, end, end]);
        let audio = decode(&data).unwrap();
        assert_eq!((audio.channels, audio.samples.len()), (2, 64));
        assert_eq!(audio.samples[0], 2);
        assert_eq!(audio.samples[1], 0);
    }

    #[test]
    fn wav_header_describes_the_samples() {
        let wav = to_wav(&Audio { channels: 2, sample_rate: 44100, samples: vec![1, -1] });
        assert_eq!(&wav[..4], b"RIFF");
        assert_eq!(u32::from_le_bytes(wav[40..44].try_into().unwrap()), 4);
        assert_eq!(u32::from_le_bytes(wav[28..32].try_into().unwrap()), 44100 * 4);
        assert_eq!(wav.len(), 48);
    }

    #[test]
    fn other_encodings_are_refused() {
        assert!(decode(b"not audio at all, really").is_err());
    }
}
