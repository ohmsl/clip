import { useQuery } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";
import { AudioDevice } from "../types/devices/AudioDevice";
import { useBackendConnectionStore } from "../state/backendConnection";
import { addToast } from "../lib/toast";

export const useAudioDevices = () => {
    const status = useBackendConnectionStore((state) => state.status);

    return {
        query: useQuery({
            queryKey: ["audio", "devices"],
            queryFn: () => {
                return invoke<Array<AudioDevice>>("list_microphone_devices");
            },
            enabled: status === "connected",
            throwOnError: (error) => {
                console.error(error);
                addToast({
                    title: "Error fetching audio devices",
                    description: error.message,
                    severity: "danger",
                });
                return true;
            },
        }),
    };
};
