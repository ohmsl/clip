import { CogIcon, LogsIcon } from "lucide-react";
import { useNavigate } from "react-router";
import { useBackendConnectionStore } from "../../state/backendConnection";
import { useCaptureStore } from "../../state/captureRuntime";
import { Button } from "../ui/button";
import {
    DropdownMenu,
    DropdownMenuContent,
    DropdownMenuItem,
    DropdownMenuTrigger,
} from "../ui/dropdown-menu";

export const ClipPanel = () => {
    const navigate = useNavigate();

    const connectionStatus = useBackendConnectionStore((state) => state.status);
    const backendError = useBackendConnectionStore((state) => state.lastError);

    const capturePhase = useCaptureStore((state) => state.capturePhase);
    const isClipping = useCaptureStore((state) => state.isClipping);
    const clipError = useCaptureStore((state) => state.clipError);
    const lastCaptureError = useCaptureStore((state) => state.lastCaptureError);
    const status = useCaptureStore((state) => state.status);
    const requestClip = useCaptureStore((state) => state.requestClip);

    const isCaptureUnavailable =
        capturePhase === "restarting" ||
        capturePhase !== "running" ||
        isClipping;
    const clipSeconds = status?.buffer_seconds ?? 30;

    const label = isClipping
        ? "CLIPPING"
        : capturePhase === "restarting"
          ? "RESTARTING"
          : "CLIP";

    const errorNote = clipError ?? lastCaptureError ?? backendError ?? null;

    return (
        <div className="flex-1 flex items-center justify-center">
            <div className="flex flex-col items-center gap-3">
                <Button
                    className="w-100 h-24 text-4xl font-semibold tracking-wide rounded-none"
                    disabled={
                        isCaptureUnavailable || connectionStatus !== "connected"
                    }
                    onClick={() => requestClip(connectionStatus)}
                >
                    {label}
                </Button>

                <div className="text-sm text-muted-foreground font-mono">
                    last {clipSeconds}s
                </div>
                {errorNote ? (
                    <div className="text-xs text-destructive font-mono">
                        {errorNote}
                    </div>
                ) : null}

                <DropdownMenu>
                    <DropdownMenuTrigger>
                        <Button
                            variant="ghost"
                            className="text-muted-foreground"
                        >
                            More
                        </Button>
                    </DropdownMenuTrigger>

                    <DropdownMenuContent>
                        <DropdownMenuItem onClick={() => navigate("log")}>
                            <LogsIcon className="w-4 h-4" />
                            Log
                        </DropdownMenuItem>
                        <DropdownMenuItem onClick={() => navigate("settings")}>
                            <CogIcon className="w-4 h-4" />
                            Settings
                        </DropdownMenuItem>
                    </DropdownMenuContent>
                </DropdownMenu>
            </div>
        </div>
    );
};
