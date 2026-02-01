import { useMemo } from "react";
import { StatusIndicator } from "../StatusIndicator";
import { useCaptureStore } from "../../state/captureRuntime";
import { useSettings } from "../../hooks/useSettings";
import { useVideoDevices } from "../../hooks/useVideoDevices";
import { useVideoEncoders } from "../../hooks/useVideoEncoders";

const formatDuration = (ms: number) => {
    const totalSeconds = Math.max(0, Math.floor(ms / 1000));
    const hours = Math.floor(totalSeconds / 3600);
    const minutes = Math.floor((totalSeconds % 3600) / 60);
    const seconds = totalSeconds % 60;

    return [
        String(hours).padStart(2, "0"),
        String(minutes).padStart(2, "0"),
        String(seconds).padStart(2, "0"),
    ].join(":");
};

const extractResolution = (label?: string | null) => {
    if (!label) {
        return null;
    }
    const direct = label.match(/(\d{3,5})\s*[xX]\s*(\d{3,5})/);
    if (direct) {
        return `${direct[1]}x${direct[2]}`;
    }
    const loose = label.match(/(\d{3,5})\D+(\d{3,5})/);
    if (loose) {
        return `${loose[1]}x${loose[2]}`;
    }
    return null;
};

const resolveResolution = (width?: number, height?: number, label?: string | null) => {
    if (width && height) {
        return `${width}x${height}`;
    }
    return extractResolution(label ?? null);
};

const formatEncoderName = (encoderId?: string | null, encoderDescription?: string | null) => {
    if (!encoderId) {
        return encoderDescription ?? "Encoder unknown";
    }
    const id = encoderId.toLowerCase();
    if (id.includes("nv") && id.includes("h264")) {
        return "NVENC H.264";
    }
    if (id.includes("qsv") || id.includes("msdk")) {
        return "Intel Quick Sync H.264";
    }
    if (id.includes("amf")) {
        return "AMD AMF H.264";
    }
    if (id.includes("vaapi")) {
        return "VAAPI H.264";
    }
    if (id.includes("mf") || id.includes("mediafoundation")) {
        return "Media Foundation H.264";
    }
    if (id.includes("x264")) {
        return "x264";
    }
    return encoderDescription ?? encoderId;
};

export const StatusBar = () => {
    const capturePhase = useCaptureStore((state) => state.capturePhase);
    const elapsedMs = useCaptureStore((state) => state.elapsedMs);

    const { query: settingsQuery } = useSettings();
    const { query: encodersQuery } = useVideoEncoders();
    const { query: videoDevicesQuery } = useVideoDevices();

    const settings = settingsQuery.data ?? null;
    const encoders = encodersQuery.data ?? [];
    const videoDevices = videoDevicesQuery.data ?? [];

    const timerLabel = useMemo(() => {
        if (capturePhase === "running") {
            return formatDuration(elapsedMs);
        }
        if (capturePhase === "restarting") {
            return "restarting";
        }
        if (capturePhase === "error") {
            return "error";
        }
        return "stopped";
    }, [capturePhase, elapsedMs]);

    const { captureConfigLabel, encoderName } = useMemo(() => {
        if (!settings) {
            return { captureConfigLabel: null, encoderName: null };
        }
        const device = videoDevices.find(
            (entry) => entry.id === settings.video_device_id,
        );
        const resolutionLabel = resolveResolution(
            device?.width,
            device?.height,
            device?.label ?? device?.id ?? null,
        );
        const encoder = encoders.find(
            (entry) => entry.id === settings.video_encoder_id,
        );
        const encoderLabel = formatEncoderName(
            encoder?.id ?? settings.video_encoder_id,
            encoder?.description ?? null,
        );

        return {
            captureConfigLabel: `${resolutionLabel ?? "Resolution unknown"} ${settings.framerate.toFixed(2)} FPS`,
            encoderName: encoderLabel,
        };
    }, [encoders, settings, videoDevices]);

    const isRunning = capturePhase === "running";

    return (
        <div className="flex justify-between items-center px-8 py-4 text-sm font-mono">
            <StatusIndicator phase={capturePhase} timerLabel={timerLabel} />

            <div className="text-right">
                {isRunning && captureConfigLabel ? (
                    <>
                        <div>{captureConfigLabel}</div>
                        <div className="text-neutral-400">
                            {encoderName ?? "Encoder unknown"}
                        </div>
                    </>
                ) : (
                    <div className="text-neutral-400">Not capturing</div>
                )}
            </div>
        </div>
    );
};
