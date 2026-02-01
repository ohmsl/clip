import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useRef } from "react";
import { LogEvent } from "../types/LogEvent";
import { useBackendConnectionStore } from "../state/backendConnection";
import { useCaptureStore, StatusResponse } from "../state/captureRuntime";

type CaptureStatusEvent = {
    status: string;
    message?: string | null;
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
            setAudioCaps(nextStatus.audio_caps ?? {});
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
