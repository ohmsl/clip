import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import { UserSettings } from "../types/UserSettings";
import { CapturePhase } from "../types/CapturePhase";
import { AudioCaps } from "../types/AudioCaps";
import { BackendConnectionStatus } from "./backendConnection";

type StatusResponse = {
    settings: UserSettings;
    buffering: boolean;
    buffer_seconds: number;
    ring_buffer_packets: number;
};

type AudioCapsState = {
    system?: AudioCaps;
    mic?: AudioCaps;
};

type CaptureStore = {
    status: StatusResponse | null;
    capturePhase: CapturePhase;
    lastCaptureError: string | null;
    startedAt: number | null;
    elapsedMs: number;
    isClipping: boolean;
    clipError: string | null;
    audioCaps: AudioCapsState;
    lastAttemptLabel: string | null;
    refreshStatus: (() => Promise<void>) | null;
    setStatus: (status: StatusResponse | null) => void;
    setCapturePhase: (phase: CapturePhase) => void;
    setLastCaptureError: (message: string | null) => void;
    setStartedAt: (value: number | null) => void;
    setElapsedMs: (value: number) => void;
    setIsClipping: (value: boolean) => void;
    setClipError: (message: string | null) => void;
    setAudioCaps: (caps: AudioCapsState) => void;
    setLastAttemptLabel: (label: string | null) => void;
    setRefreshStatus: (fn: (() => Promise<void>) | null) => void;
    requestClip: (connectionStatus: BackendConnectionStatus) => Promise<void>;
};

export const useCaptureStore = create<CaptureStore>((set, get) => ({
    status: null,
    capturePhase: "unknown",
    lastCaptureError: null,
    startedAt: null,
    elapsedMs: 0,
    isClipping: false,
    clipError: null,
    audioCaps: {},
    lastAttemptLabel: null,
    refreshStatus: null,
    setStatus: (status) => set({ status }),
    setCapturePhase: (capturePhase) => set({ capturePhase }),
    setLastCaptureError: (message) => set({ lastCaptureError: message }),
    setStartedAt: (startedAt) => set({ startedAt }),
    setElapsedMs: (elapsedMs) => set({ elapsedMs }),
    setIsClipping: (isClipping) => set({ isClipping }),
    setClipError: (clipError) => set({ clipError }),
    setAudioCaps: (audioCaps) => set({ audioCaps }),
    setLastAttemptLabel: (lastAttemptLabel) => set({ lastAttemptLabel }),
    setRefreshStatus: (refreshStatus) => set({ refreshStatus }),
    requestClip: async (connectionStatus) => {
        const {
            capturePhase,
            isClipping,
            refreshStatus,
            setIsClipping,
            setClipError,
        } = get();

        if (connectionStatus !== "connected") {
            return;
        }
        if (capturePhase !== "running" || isClipping) {
            return;
        }

        setIsClipping(true);
        setClipError(null);
        try {
            await invoke("clip");
            if (refreshStatus) {
                await refreshStatus();
            }
        } catch (error) {
            setClipError(
                error instanceof Error ? error.message : "Failed to create clip",
            );
        } finally {
            setIsClipping(false);
        }
    },
}));

export type { StatusResponse, AudioCapsState };
