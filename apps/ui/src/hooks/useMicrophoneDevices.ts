import { addToast } from "../lib/toast";
import { useQuery } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";
import { useBackendConnectionStore } from "../state/backendConnection";
import { AudioDevice } from "../types/devices/AudioDevice";

export const useMicrophoneDevices = () => {
    const status = useBackendConnectionStore((state) => state.status);

    return {
        query: useQuery({
            queryKey: ["audio", "microphones"],
            queryFn: () => invoke<Array<AudioDevice>>("list_microphone_devices"),
            select: (devices) => {
                const seen = new Set<string>();
                return devices.filter((device) => {
                    if (seen.has(device.id)) {
                        return false;
                    }
                    seen.add(device.id);
                    return true;
                });
            },
            enabled: status === "connected",
            throwOnError: (error) => {
                console.error(error);
                addToast({
                    title: "Error fetching microphones",
                    description: error.message,
                    severity: "danger",
                });
                return true;
            },
        }),
    };
};
