import { useSettings } from "../../hooks/useSettings";
import { useCaptureStore } from "../../state/captureRuntime";
import { AudioCaps } from "../../types/AudioCaps";

type AudioLineInput = {
    enabled: boolean;
    reason: string | null;
    caps?: AudioCaps;
};

const formatRate = (rate?: number) => {
    if (!rate) {
        return null;
    }
    const khz = rate / 1000;
    const label = Number.isInteger(khz) ? `${khz}` : khz.toFixed(1);
    return `${label}kHz`;
};

const formatChannels = (channels?: number) => {
    if (!channels) {
        return null;
    }
    if (channels === 1) {
        return "mono";
    }
    return `${channels}ch`;
};

const formatThroughput = (bytes?: number, durationMs?: number) => {
    if (!bytes || !durationMs || durationMs <= 0) {
        return null;
    }
    const mbps = (bytes * 8) / (durationMs * 1000);
    return `${mbps.toFixed(1)} Mbps`;
};

const buildAudioLine = (label: string, input: AudioLineInput) => {
    if (!input.enabled) {
        return `${label} disabled${input.reason ? ` (${input.reason})` : ""}`;
    }

    const rate = formatRate(input.caps?.rate);
    const channels = formatChannels(input.caps?.channels);
    if (rate && channels) {
        return `${label} ${rate} / ${channels}`;
    }
    return `${label} format unknown`;
};

const buildReason = (
    settingsAvailable: boolean,
    enabled: boolean,
    autoDisabled: boolean,
) => {
    if (!settingsAvailable) {
        return null;
    }
    if (!enabled) {
        return "settings";
    }
    if (autoDisabled) {
        return "fallback";
    }
    return null;
};

export const DiagnosticsBar = () => {
    const { query: settingsQuery } = useSettings();
    const settings = settingsQuery.data ?? null;

    const audioCaps = useCaptureStore((state) => state.audioCaps);
    const lastAttemptLabel = useCaptureStore((state) => state.lastAttemptLabel);
    const status = useCaptureStore((state) => state.status);

    const systemEnabled = settings?.system_audio_enabled ?? false;
    const micEnabled = !!settings?.mic_device_id;

    const systemAutoDisabled =
        systemEnabled && lastAttemptLabel?.includes("system disabled")
            ? true
            : false;
    const micAutoDisabled =
        micEnabled && lastAttemptLabel?.includes("mic disabled") ? true : false;

    const systemAudioLine = buildAudioLine("SYS •", {
        enabled: systemEnabled && !systemAutoDisabled,
        reason: buildReason(!!settings, systemEnabled, systemAutoDisabled),
        caps: audioCaps.system,
    });

    const micAudioLine = buildAudioLine("MIC •", {
        enabled: micEnabled && !micAutoDisabled,
        reason: buildReason(!!settings, micEnabled, micAutoDisabled),
        caps: audioCaps.mic,
    });

    const bufferLabel =
        typeof status?.buffer_seconds === "number"
            ? `${status.buffer_seconds}s`
            : "--";
    const throughputLabel = formatThroughput(
        status?.ring_buffer_bytes,
        status?.ring_buffer_duration_ms,
    );

    return (
        <div className="flex justify-between px-8 py-4 text-sm font-mono text-neutral-400 border-t border-neutral-800">
            <div className="space-y-1">
                <div>{systemAudioLine}</div>
                <div>{micAudioLine}</div>
            </div>

            <div className="text-right space-y-1">
                <div>BUF • {bufferLabel}</div>
                <div>THR • {throughputLabel ?? "--"}</div>
            </div>
        </div>
    );
};
