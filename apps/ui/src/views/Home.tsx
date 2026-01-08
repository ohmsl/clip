import { useCaptureController } from "../hooks/useCaptureController";
import { ClipPanel } from "../components/homeV2/ClipPanel";
import { DiagnosticsBar } from "../components/homeV2/DiagnosticsBar";
import { StatusBar } from "../components/homeV2/StatusBar";

export const Home = () => {
    useCaptureController();

    return (
        <main className="flex flex-col h-dvh gap-4 p-8">
            <div className="h-screen flex flex-col">
                <StatusBar />
                <ClipPanel />
                <DiagnosticsBar />
            </div>
        </main>
    );
};
