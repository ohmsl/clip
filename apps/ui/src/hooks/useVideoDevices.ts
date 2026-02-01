import { addToast } from "../lib/toast";
import { useQuery } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";
import { useBackendConnectionStore } from "../state/backendConnection";
import { VideoDevice } from "../types/devices/VideoDevice";

export const useVideoDevices = () => {
    const status = useBackendConnectionStore((state) => state.status);

    return {
        query: useQuery({
            queryKey: ["video", "devices"],
            queryFn: () => {
                return invoke<Array<VideoDevice>>("list_video_devices");
            },
            enabled: status === "connected",
            throwOnError: (error) => {
                console.error(error);
                addToast({
                    title: "Error fetching video devices",
                    description: error.message,
                    severity: "danger",
                });
                return true;
            },
        }),
    };
};
