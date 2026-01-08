import {
    Button,
    Dropdown,
    DropdownItem,
    DropdownMenu,
    DropdownTrigger,
} from "@heroui/react";
import { CogIcon } from "lucide-react";
import { useNavigate } from "react-router";
import { useBackendConnectionStore } from "../../state/backendConnection";
import { useCaptureStore } from "../../state/captureRuntime";

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
                    color="primary"
                    className="w-100 h-24 text-default-900 text-4xl font-semibold tracking-wide rounded-none"
                    disabled={
                        isCaptureUnavailable || connectionStatus !== "connected"
                    }
                    onPress={() => requestClip(connectionStatus)}
                >
                    {label}
                </Button>

                <div className="text-sm text-neutral-400 font-mono">
                    last {clipSeconds}s
                </div>
                {errorNote ? (
                    <div className="text-xs text-red-400 font-mono">
                        {errorNote}
                    </div>
                ) : null}

                <Dropdown>
                    <DropdownTrigger>
                        <Button variant="light" className="text-neutral-400">
                            More
                        </Button>
                    </DropdownTrigger>

                    <DropdownMenu>
                        <DropdownItem
                            key="settings"
                            startContent={<CogIcon className="w-5 h-5 mr-2" />}
                            onPress={() => navigate("settings")}
                        >
                            Settings
                        </DropdownItem>
                    </DropdownMenu>
                </Dropdown>
            </div>
        </div>
    );
};
