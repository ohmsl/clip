import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useRef } from "react";
import { AudioCaps } from "../types/AudioCaps";
import { LogEvent } from "../types/LogEvent";
import { UserSettings } from "../types/UserSettings";
import { useBackendConnectionStore } from "../state/backendConnection";
import {
    useCaptureStore,
    StatusResponse,
    AudioCapsState,
} from "../state/captureRuntime";

type CaptureStatusEvent = {
    status: string;
    message?: string | null;
};

const parseCapsLog = (message: string) => {
    const match = message.match(/source\s+(\d+)\s+negotiated caps:\s+(.*)$/);
    if (!match) {
        return null;
    }
    const index = Number(match[1]);
    if (Number.isNaN(index)) {
        return null;
    }

    let raw = match[2].trim();
    if (raw.startsWith("Some(")) {
        raw = raw.replace(/^Some\(\"?/, "").replace(/\"\)?$/, "");
    }

    if (raw === "None") {
        return { index, caps: null as AudioCaps | null };
    }

    const rateMatch = raw.match(/rate=\(int\)(\d+)/);
    const channelsMatch = raw.match(/channels=\(int\)(\d+)/);

    const caps: AudioCaps = {
        rate: rateMatch ? Number(rateMatch[1]) : undefined,
        channels: channelsMatch ? Number(channelsMatch[1]) : undefined,
        raw,
    };

    return { index, caps };
};

const parseCoercedCapsLog = (message: string) => {
    const match = message.match(
        /^(system|mic) audio coerced to mix caps: rate=(\d+), channels=(\d+)/,
    );
    if (!match) {
        return null;
    }
    const source = match[1] === "system" ? "system" : "mic";
    const rate = Number(match[2]);
    const channels = Number(match[3]);
    if (Number.isNaN(rate) || Number.isNaN(channels)) {
        return null;
    }
    const caps: AudioCaps = {
        rate,
        channels,
        raw: message,
    };
    return { source, caps };
};

const mapAudioIndexToSource = (index: number, settings: UserSettings) => {
    const systemEnabled = settings.system_audio_enabled;
    const micEnabled = !!settings.mic_device_id;

    if (systemEnabled && micEnabled) {
        return index === 0 ? "system" : index === 1 ? "mic" : null;
    }
    if (systemEnabled && !micEnabled) {
        return index === 0 ? "system" : null;
    }
    if (!systemEnabled && micEnabled) {
        return index === 0 ? "mic" : null;
    }
    return null;
};

const applyAudioCapsUpdate = (
    settings: UserSettings | null,
    parsed: { index: number; caps: AudioCaps | null },
    update: (caps: AudioCapsState) => void,
) => {
    if (!settings) {
        return;
    }
    const source = mapAudioIndexToSource(parsed.index, settings);
    if (!source) {
        return;
    }
    update({ [source]: parsed.caps ?? undefined });
};

export const useCaptureController = () => {
    const connectionStatus = useBackendConnectionStore((state) => state.status);

    const capturePhase = useCaptureStore((state) => state.capturePhase);
    const startedAt = useCaptureStore((state) => state.startedAt);
    const setStatus = useCaptureStore((state) => state.setStatus);
    const setCapturePhase = useCaptureStore((state) => state.setCapturePhase);
    const setLastCaptureError = useCaptureStore(
        (state) => state.setLastCaptureError,
    );
    const setStartedAt = useCaptureStore((state) => state.setStartedAt);
    const setElapsedMs = useCaptureStore((state) => state.setElapsedMs);
    const setAudioCaps = useCaptureStore((state) => state.setAudioCaps);
    const setLastAttemptLabel = useCaptureStore(
        (state) => state.setLastAttemptLabel,
    );
    const setRefreshStatus = useCaptureStore((state) => state.setRefreshStatus);

    const mountedAt = useRef(Date.now());

    const refreshStatus = useCallback(async () => {
        if (connectionStatus !== "connected") {
            return;
        }
        try {
            const nextStatus = await invoke<StatusResponse>("get_status");
            setStatus(nextStatus);
            const state = useCaptureStore.getState();
            if (state.capturePhase === "unknown" || state.capturePhase === "stopped") {
                if (nextStatus.buffering) {
                    setCapturePhase("running");
                    if (!state.startedAt) {
                        setStartedAt(Date.now());
                    }
                } else {
                    setCapturePhase("stopped");
                }
            }
        } catch (error) {
            setLastCaptureError(
                error instanceof Error ? error.message : "Failed to fetch status",
            );
        }
    }, [
        connectionStatus,
        setCapturePhase,
        setLastCaptureError,
        setStartedAt,
        setStatus,
    ]);

    useEffect(() => {
        setRefreshStatus(refreshStatus);
        return () => {
            setRefreshStatus(null);
        };
    }, [refreshStatus, setRefreshStatus]);

    useEffect(() => {
        refreshStatus();
    }, [refreshStatus]);

    useEffect(() => {
        if (connectionStatus !== "connected") {
            return;
        }
        const intervalMs = capturePhase === "running" ? 1000 : 5000;
        const interval = setInterval(() => {
            refreshStatus();
        }, intervalMs);
        return () => clearInterval(interval);
    }, [capturePhase, connectionStatus, refreshStatus]);

    useEffect(() => {
        let active = true;
        let unlisten: null | (() => void) = null;

        listen<CaptureStatusEvent>("capture-status", (event) => {
            if (!active) {
                return;
            }
            const nextStatus = event.payload.status;
            if (nextStatus === "running") {
                setCapturePhase("running");
                setStartedAt(Date.now());
                setLastCaptureError(null);
            } else if (nextStatus === "stopped") {
                setCapturePhase("stopped");
                setStartedAt(null);
            } else if (nextStatus === "error") {
                setCapturePhase("error");
                setStartedAt(null);
                setLastCaptureError(event.payload.message ?? "Capture error");
            }
            refreshStatus();
        })
            .then((unlistenFn) => {
                if (!active) {
                    unlistenFn();
                    return;
                }
                unlisten = unlistenFn;
            })
            .catch((error) => {
                if (!active) {
                    return;
                }
                setLastCaptureError(
                    error instanceof Error
                        ? error.message
                        : "Event stream unavailable",
                );
            });

        return () => {
            active = false;
            if (unlisten) {
                unlisten();
            }
        };
    }, [refreshStatus, setCapturePhase, setLastCaptureError, setStartedAt]);

    useEffect(() => {
        if (capturePhase !== "running" || !startedAt) {
            setElapsedMs(0);
            return;
        }
        setElapsedMs(Date.now() - startedAt);
        const interval = setInterval(() => {
            const state = useCaptureStore.getState();
            if (state.capturePhase !== "running" || !state.startedAt) {
                setElapsedMs(0);
                return;
            }
            setElapsedMs(Date.now() - state.startedAt);
        }, 1000);
        return () => clearInterval(interval);
    }, [capturePhase, setElapsedMs, startedAt]);

    useEffect(() => {
        if (
            capturePhase === "restarting" ||
            capturePhase === "stopped" ||
            capturePhase === "error"
        ) {
            setAudioCaps({});
            setLastAttemptLabel(null);
        }
    }, [capturePhase, setAudioCaps, setLastAttemptLabel]);

    useEffect(() => {
        let active = true;
        let unlisten: null | (() => void) = null;

        const handleLog = (log: LogEvent) => {
            if (log.source === "capture") {
                if (log.message.includes("stopping existing pipeline")) {
                    const logTime = Date.parse(log.timestamp);
                    if (Number.isNaN(logTime) || logTime >= mountedAt.current) {
                        setCapturePhase("restarting");
                        setStartedAt(null);
                        setLastCaptureError(null);
                    }
                }
            }

            if (log.source === "audio" && log.message.startsWith("capture attempt:")) {
                const label = log.message.replace("capture attempt:", "").trim();
                setLastAttemptLabel(label);
            }

            if (log.source === "audio" && log.message.includes("negotiated caps")) {
                const parsed = parseCapsLog(log.message);
                if (!parsed) {
                    return;
                }
                const settings = useCaptureStore.getState().status?.settings ?? null;
                applyAudioCapsUpdate(settings, parsed, (update) =>
                    setAudioCaps({
                        ...useCaptureStore.getState().audioCaps,
                        ...update,
                    }),
                );
            }

            if (log.source === "audio" && log.message.includes("audio coerced to mix caps")) {
                const parsed = parseCoercedCapsLog(log.message);
                if (!parsed) {
                    return;
                }
                setAudioCaps({
                    ...useCaptureStore.getState().audioCaps,
                    [parsed.source]: parsed.caps,
                });
            }
        };

        invoke<Array<LogEvent>>("get_recent_logs")
            .then((events) => {
                if (!active) {
                    return;
                }
                events.forEach(handleLog);
            })
            .catch(() => {
                // ignore
            });

        listen<LogEvent>("capture-log", (event) => {
            if (!active) {
                return;
            }
            handleLog(event.payload);
        })
            .then((unlistenFn) => {
                if (!active) {
                    unlistenFn();
                    return;
                }
                unlisten = unlistenFn;
            })
            .catch(() => {
                // ignore
            });

        return () => {
            active = false;
            if (unlisten) {
                unlisten();
            }
        };
    }, [setAudioCaps, setCapturePhase, setLastAttemptLabel, setLastCaptureError, setStartedAt]);
};
