import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { BrowserRouter, Route, Routes } from "react-router";
import { Toaster } from "sonner";
import { LogView } from "./components/LogView";
import { SettingsView } from "./components/SettingsView";
import "./globals.css";
import { useBackendConnection } from "./hooks/useBackendConnection";
import { Home } from "./views/Home";

function App() {
    useBackendConnection();

    return (
        <QueryClientProvider client={new QueryClient()}>
            <div className="root min-h-dvh bg-background text-foreground">
                <Toaster richColors theme="dark" />
                <BrowserRouter>
                    <Routes>
                        <Route path="/" element={<Home />} />
                        <Route path="/log" element={<LogView />} />
                        <Route path="/settings" element={<SettingsView />} />
                    </Routes>
                </BrowserRouter>
            </div>
        </QueryClientProvider>
    );
}

export default App;
