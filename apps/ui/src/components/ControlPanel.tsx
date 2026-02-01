import { Button } from "./ui/button";
import { Separator } from "./ui/separator";
import { invoke } from "@tauri-apps/api/core";
import { openPath } from "@tauri-apps/plugin-opener";
import { useBackendConnectionStore } from "../state/backendConnection";

export const ControlPanel = () => {
    const connectionStatus = useBackendConnectionStore((state) => state.status);

    function handlePressStatus() {
        invoke("get_status");
    }

    function handlePressClip() {
        invoke("clip");
    }

    async function handlePressListClips() {
        const clipsDir = await invoke<string>("get_clips_dir");
        openPath(clipsDir);
    }

    function handlePressShutdown() {
        invoke("stop_capture");
    }

    return (
        <div className="flex flex-col gap-3 rounded-lg border border-border bg-card p-4">
            <Button
                onClick={handlePressClip}
                disabled={connectionStatus !== "connected"}
            >
                Clip
            </Button>

            <Button
                variant="secondary"
                onClick={handlePressStatus}
                disabled={connectionStatus !== "connected"}
            >
                Status
            </Button>

            <Button
                variant="secondary"
                onClick={handlePressListClips}
                disabled={connectionStatus !== "connected"}
            >
                View Clips
            </Button>

            <Separator />

            <Button
                variant="destructive"
                onClick={handlePressShutdown}
                disabled={connectionStatus !== "connected"}
            >
                Stop Capture
            </Button>
        </div>
    );
};
