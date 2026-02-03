export type UserSettings = {
    video_device_id: string;
    system_audio_enabled: boolean;
    system_audio_volume: number;
    mic_device_id?: string | null;
    mic_volume: number;
    video_encoder_id: string;
    framerate: number;
    bitrate_kbps: number;
    clips_dir: string;
    shortcuts: {
        clip: string;
    };
};
