import { useMutation, useQuery } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";
import { useBackendConnectionStore } from "../state/backendConnection";
import { UserSettings } from "../types/UserSettings";
import { addToast } from "../lib/toast";

export const useSettings = () => {
    const status = useBackendConnectionStore((state) => state.status);

    return {
        query: useQuery({
            queryKey: ["settings"],
            queryFn: () => invoke<UserSettings>("get_settings"),
            enabled: status === "connected",
            throwOnError: (error) => {
                console.error(error);
                addToast({
                    title: "Error fetching settings",
                    description: error.message,
                    severity: "danger",
                });
                return true;
            },
        }),
        mutation: useMutation({
            mutationFn: (settings: UserSettings) =>
                invoke<UserSettings>("update_settings", {
                    newSettings: settings,
                }),
            onError: (error) => {
                console.error(error);
                addToast({
                    title: "Error updating settings",
                    description: error.message,
                    severity: "danger",
                });
            },
        }),
    };
};
