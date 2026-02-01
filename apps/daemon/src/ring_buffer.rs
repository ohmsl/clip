use std::collections::VecDeque;

/// A single encoded media packet with a presentation timestamp.
/// `pts_ms` must be monotonically increasing.
#[derive(Debug, Clone)]
pub struct Packet {
    pub pts_ms: u64,
    pub data: Vec<u8>,
}

pub struct RingBuffer {
    max_duration_ms: u64,
    packets: VecDeque<Packet>,
    keyframes: VecDeque<u64>,
}

impl RingBuffer {
    pub fn new(max_duration_ms: u64) -> Self {
        Self {
            max_duration_ms,
            packets: VecDeque::new(),
            keyframes: VecDeque::new(),
        }
    }

    pub fn push(&mut self, packet: Packet) {
        self.packets.push_back(packet);
        self.evict_old_packets();
    }

    pub fn push_keyframe_pts(&mut self, pts_ms: u64) {
        if self
            .keyframes
            .back()
            .map(|last| *last < pts_ms)
            .unwrap_or(true)
        {
            self.keyframes.push_back(pts_ms);
        }
    }

    fn evict_old_packets(&mut self) {
        let Some(newest) = self.packets.back() else {
            return;
        };

        let newest_pts = newest.pts_ms;

        while let Some(oldest) = self.packets.front() {
            if newest_pts.saturating_sub(oldest.pts_ms) > self.max_duration_ms {
                self.packets.pop_front();
            } else {
                break;
            }
        }

        if let Some(oldest) = self.packets.front() {
            while let Some(keyframe) = self.keyframes.front() {
                if *keyframe < oldest.pts_ms {
                    self.keyframes.pop_front();
                } else {
                    break;
                }
            }
        } else {
            self.keyframes.clear();
        }
    }

    // Return a snapshot of the current packets in the buffer
    pub fn snapshot(&self) -> Vec<Packet> {
        self.packets.iter().cloned().collect()
    }

    pub fn len(&self) -> usize {
        self.packets.len()
    }

    pub fn total_bytes(&self) -> u64 {
        self.packets
            .iter()
            .map(|packet| packet.data.len() as u64)
            .sum()
    }

    pub fn is_empty(&self) -> bool {
        self.packets.is_empty()
    }

    pub fn duration_ms(&self) -> u64 {
        match (self.packets.front(), self.packets.back()) {
            (Some(first), Some(last)) => last.pts_ms.saturating_sub(first.pts_ms),
            _ => 0,
        }
    }

    pub fn drain_from_keyframe(&mut self) -> Vec<Packet> {
        let keyframe_start = self.keyframes.front().cloned();
        let packets: Vec<Packet> = self.packets.iter().cloned().collect();
        self.clear();

        if let Some(start) = keyframe_start {
            let has_packet_after = packets
                .last()
                .map(|packet| packet.pts_ms >= start)
                .unwrap_or(false);

            if has_packet_after {
                return packets
                    .into_iter()
                    .filter(|packet| packet.pts_ms >= start)
                    .collect();
            }
        }

        packets
    }

    pub fn clear(&mut self) {
        self.packets.clear();
        self.keyframes.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn packet(pts_ms: u64) -> Packet {
        Packet {
            pts_ms,
            data: vec![0; 10],
        }
    }

    #[test]
    fn pushes_and_keeps_packets_within_duration() {
        let mut buffer = RingBuffer::new(3000);

        buffer.push(packet(0));
        buffer.push(packet(1000));
        buffer.push(packet(2000));
        buffer.push(packet(3000));

        assert_eq!(buffer.len(), 4);
        assert_eq!(buffer.duration_ms(), 3000);
    }

    #[test]
    fn evicts_packets_outside_duration() {
        let mut buffer = RingBuffer::new(2000);

        buffer.push(packet(0));
        buffer.push(packet(1000));
        buffer.push(packet(2000));
        buffer.push(packet(3000));

        // 3000 - 0 = 3000 > 2000 → evict
        assert_eq!(buffer.len(), 3);
        assert_eq!(buffer.snapshot()[0].pts_ms, 1000);
    }

    #[test]
    fn snapshot_preserves_order() {
        let mut buffer = RingBuffer::new(5000);

        buffer.push(packet(10));
        buffer.push(packet(20));
        buffer.push(packet(30));

        let snapshot = buffer.snapshot();

        assert_eq!(snapshot[0].pts_ms, 10);
        assert_eq!(snapshot[1].pts_ms, 20);
        assert_eq!(snapshot[2].pts_ms, 30);
    }
}
