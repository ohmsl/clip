import { format } from "date-fns";
import { ArrowLeftIcon } from "lucide-react";
import { useNavigate } from "react-router";
import { useDaemonLogs } from "../hooks/useDaemonLogs";
import { LogEvent } from "../types/LogEvent";
import { Button } from "./ui/button";
import { ScrollArea } from "./ui/scroll-area";

const levelClass: Record<LogEvent["level"], string> = {
    error: "text-destructive",
    warning: "text-amber-400",
    info: "text-sky-400",
    debug: "text-muted-foreground",
};

export const LogView = () => {
    const navigate = useNavigate();
    const logs = useDaemonLogs();

    return (
        <section className="min-h-dvh flex flex-col gap-6 p-8">
            <div className="flex items-center gap-4">
                <Button
                    variant="ghost"
                    size="icon"
                    onClick={() => navigate("/")}
                    aria-label="Back"
                >
                    <ArrowLeftIcon />
                </Button>
                <div className="flex-1">
                    <h1 className="text-2xl font-semibold">Capture Log</h1>
                    <p className="text-sm text-muted-foreground">
                        Live diagnostics from the capture pipeline.
                    </p>
                </div>
            </div>

            <ScrollArea className="max-h-[70vh] font-mono">
                {logs.length === 0 ? (
                    <div className="text-muted-foreground pt-4">
                        No logs yet.
                    </div>
                ) : (
                    logs.map((log, index) => (
                        <div key={`${log.timestamp}-${index}`} className="py-1">
                            <span className="text-muted-foreground">
                                {format(log.timestamp, "yyyy-MM-dd HH:mm:ss")}
                            </span>{" "}
                            <span className={levelClass[log.level]}>
                                [{log.source}]
                            </span>{" "}
                            <span>{log.message}</span>
                        </div>
                    ))
                )}
            </ScrollArea>
        </section>
    );
};
