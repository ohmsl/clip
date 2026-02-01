import { VideoDeviceCapabilities } from "./VideoDeviceCapabilities";
import { VideoDeviceKind } from "./VideoDeviceKind";

export type VideoDevice = {
    id: string;
    label: string;
    kind: VideoDeviceKind;
    width?: number;
    height?: number;
    capabilities: VideoDeviceCapabilities;
};
