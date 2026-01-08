import { ToastProvider } from "@heroui/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { BrowserRouter, Route, Routes } from "react-router";
import { SettingsView } from "./components/SettingsView";
import "./globals.css";
import { useBackendConnection } from "./hooks/useBackendConnection";
import { Home } from "./views/Home";

function App() {
    useBackendConnection();

    return (
        <QueryClientProvider client={new QueryClient()}>
            <div className="bg-background">
                <ToastProvider />

                <BrowserRouter>
                    <Routes>
                        <Route path="/" element={<Home />} />
                        <Route path="/settings" element={<SettingsView />} />
                    </Routes>
                </BrowserRouter>
            </div>
        </QueryClientProvider>
    );
}

export default App;
