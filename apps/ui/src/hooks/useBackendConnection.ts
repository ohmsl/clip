import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useRef } from "react";
import { useBackendConnectionStore } from "../state/backendConnection";
import { addToast } from "../lib/toast";

type CaptureStatusEvent = {
    status: string;
    message?: string | null;
};

type ShortcutErrorEvent = {
    message: string;
};

export const useBackendConnection = () => {
    const { setStatus, setLastError } = useBackendConnectionStore();
    const status = useBackendConnectionStore((state) => state.status);
    const lastError = useBackendConnectionStore((state) => state.lastError);
    const unlistenRef = useRef<null | (() => void)>(null);

    useEffect(() => {
        let active = true;

        setStatus("connecting");

        invoke("get_status")
            .then(() => {
                if (!active) {
                    return;
                }
                setStatus("connected");
                setLastError(null);
            })
            .catch((error) => {
                if (!active) {
                    return;
                }
                setStatus("disconnected");
                setLastError(
                    error instanceof Error ? error.message : "Connection failed",
                );
            });

        listen<CaptureStatusEvent>("capture-status", (event) => {
            if (!active) {
                return;
            }

            if (event.payload.status === "error") {
                setLastError(event.payload.message ?? "Capture error");
            } else {
                setLastError(null);
            }
            setStatus("connected");
        })
            .then((unlisten) => {
                if (!active) {
                    unlisten();
                    return;
                }
                unlistenRef.current = unlisten;
            })
            .catch((error) => {
                if (!active) {
                    return;
                }
                setStatus("disconnected");
                setLastError(
                    error instanceof Error ? error.message : "Event stream failed",
                );
            });

        listen<ShortcutErrorEvent>("shortcut-error", (event) => {
            addToast({
                title: "Shortcut unavailable",
                description: event.payload.message,
                severity: "warning",
            });
        }).catch((error) => {
            if (!active) {
                return;
            }
            setStatus("disconnected");
            setLastError(
                error instanceof Error ? error.message : "Event stream failed",
            );
        });

        return () => {
            active = false;
            if (unlistenRef.current) {
                unlistenRef.current();
                unlistenRef.current = null;
            }
        };
    }, [setLastError, setStatus]);

    return {
        status,
        lastError,
    };
};
