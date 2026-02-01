import { useMemo } from "react";
import { CapturePhase } from "../types/CapturePhase";

type StatusIndicatorProps = {
    phase?: CapturePhase;
    timerLabel?: string;
};

export const StatusIndicator = ({
    phase = "unknown",
    timerLabel = "--:--:--",
}: StatusIndicatorProps) => {
    const { label, className } = useMemo(() => {
        switch (phase) {
            case "running":
                return { label: "REC", className: "text-foreground" };
            case "restarting":
                return { label: "RST", className: "text-yellow-500" };
            case "error":
                return { label: "ERR", className: "text-red-400" };
            case "stopped":
            case "unknown":
            default:
                return { label: "OFF", className: "text-neutral-500" };
        }
    }, [phase]);

    return (
        <div className="flex items-center gap-3">
            {phase === "running" && (
                <span className="text-red-600 animate-pulse">●</span>
            )}
            <span className={className}>{label}</span>
            <span className="text-neutral-400">{timerLabel}</span>
        </div>
    );
};
