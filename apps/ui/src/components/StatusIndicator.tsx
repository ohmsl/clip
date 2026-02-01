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
                return { label: "RST", className: "text-amber-400" };
            case "error":
                return { label: "ERR", className: "text-destructive" };
            case "stopped":
            case "unknown":
            default:
                return { label: "OFF", className: "text-muted-foreground" };
        }
    }, [phase]);

    return (
        <div className="flex items-center gap-3">
            {phase === "running" && (
            <span className="text-destructive animate-pulse">●</span>
        )}
        <span className={className}>{label}</span>
        <span className="text-muted-foreground">{timerLabel}</span>
    </div>
    );
};
