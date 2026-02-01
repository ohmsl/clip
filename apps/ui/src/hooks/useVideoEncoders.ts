import { addToast } from "../lib/toast";
import { useQuery } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";
import { useBackendConnectionStore } from "../state/backendConnection";
import { VideoEncoder } from "../types/VideoEncoder";

export const useVideoEncoders = () => {
    const status = useBackendConnectionStore((state) => state.status);

    return {
        query: useQuery({
            queryKey: ["video", "encoders"],
            queryFn: () => invoke<Array<VideoEncoder>>("list_video_encoders"),
            enabled: status === "connected",
            throwOnError: (error) => {
                console.error(error);
                addToast({
                    title: "Error fetching encoders",
                    description: error.message,
                    severity: "danger",
                });
                return true;
            },
        }),
    };
};
