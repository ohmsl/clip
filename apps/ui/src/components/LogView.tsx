import { ScrollShadow } from "@heroui/react";
import { format } from "date-fns";
import { LogsIcon } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { useDaemonLogs } from "../hooks/useDaemonLogs";
import { LogEvent } from "../types/LogEvent";

const levelClass: Record<LogEvent["level"], string> = {
    error: "text-danger",
    warning: "text-warning",
    info: "text-blue-400",
    debug: "text-default-500",
};

export const LogView = () => {
    const logs = useDaemonLogs();
    const containerRef = useRef<HTMLDivElement | null>(null);
    const [autoScroll, setAutoScroll] = useState(true);

    useEffect(() => {
        if (!autoScroll) {
            return;
        }
        const container = containerRef.current;
        if (container) {
            container.scrollTop = container.scrollHeight;
        }
    }, [autoScroll, logs.length]);

    const handleScroll = () => {
        const container = containerRef.current;
        if (!container) {
            return;
        }

        const threshold = 24;
        const atBottom =
            container.scrollHeight -
                container.scrollTop -
                container.clientHeight <
            threshold;
        setAutoScroll(atBottom);
    };

    return (
        <div className="p-4">
            <div className="flex flex-col rounded-large border-1 border-divider overflow-hidden">
                <div className="flex items-center gap-4 bg-content1 p-4">
                    <LogsIcon size={21} className="text-default-500" />
                    <h6 className="text-medium grow">Capture Log</h6>
                    <span className="text-xs text-default-500">
                        {logs.length} lines
                    </span>
                </div>

                <ScrollShadow
                    ref={containerRef}
                    onScroll={handleScroll}
                    className="overflow-y-auto whitespace-pre-wrap p-4 font-mono text-xs"
                >
                    {logs.length === 0 ? (
                        <div className="text-default-500">No logs yet.</div>
                    ) : (
                        logs.map((log, index) => (
                            <div
                                key={`${log.timestamp}-${index}`}
                                className="py-1"
                            >
                                <span className="text-default-500">
                                    {format(
                                        log.timestamp,
                                        "yyyy-MM-dd HH:mm:ss",
                                    )}
                                </span>{" "}
                                <span className={levelClass[log.level]}>
                                    [{log.source}]
                                </span>{" "}
                                <span>{log.message}</span>
                            </div>
                        ))
                    )}
                </ScrollShadow>
            </div>
        </div>
    );
};
