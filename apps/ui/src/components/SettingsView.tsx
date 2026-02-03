import { useQueryClient } from "@tanstack/react-query";
import { open } from "@tauri-apps/plugin-dialog";
import {
    ArrowLeftIcon,
    BinaryIcon,
    DatabaseIcon,
    TvMinimalPlayIcon,
    Volume2Icon,
} from "lucide-react";
import { useEffect, useState } from "react";
import { useNavigate } from "react-router";
import { useMicrophoneDevices } from "../hooks/useMicrophoneDevices";
import { useSettings } from "../hooks/useSettings";
import { useVideoDevices } from "../hooks/useVideoDevices";
import { useVideoEncoders } from "../hooks/useVideoEncoders";
import { addToast } from "../lib/toast";
import { useBackendConnectionStore } from "../state/backendConnection";
import { UserSettings } from "../types/UserSettings";
import { SectionTitle } from "./SectionTitle";
import { Button } from "./ui/button";
import { Input } from "./ui/input";
import {
    Select,
    SelectContent,
    SelectItem,
    SelectTrigger,
    SelectValue,
} from "./ui/select";
import { Separator } from "./ui/separator";
import { Slider } from "./ui/slider";
import { Switch } from "./ui/switch";

export const SettingsView = () => {
    const navigate = useNavigate();

    const {
        query: { data: videoDevices },
    } = useVideoDevices();

    const {
        query: { data: microphoneDevices },
    } = useMicrophoneDevices();

    const {
        query: { data: encoders },
    } = useVideoEncoders();

    const {
        query: { data: settings },
        mutation: settingsMutation,
    } = useSettings();

    const queryClient = useQueryClient();
    const connectionStatus = useBackendConnectionStore((state) => state.status);

    const [form, setForm] = useState<UserSettings | null>(null);
    const [isRecordingShortcut, setIsRecordingShortcut] = useState(false);

    useEffect(() => {
        if (settings) {
            setForm(settings);
        }
    }, [settings]);
    const updateForm = <K extends keyof UserSettings>(
        key: K,
        value: UserSettings[K],
    ) => {
        setForm((prev) => (prev ? { ...prev, [key]: value } : prev));
    };

    const handleApplySettings = () => {
        if (!form) {
            return;
        }

        if (connectionStatus !== "connected") {
            addToast({
                title: "Backend offline",
                description: "Start the backend to apply settings.",
                severity: "danger",
            });
            return;
        }

        settingsMutation.mutate(form, {
            onSuccess: (data) => {
                queryClient.setQueryData(["settings"], data);
                addToast({
                    title: "Settings updated",
                    severity: "success",
                });
            },
        });
    };

    const formatShortcut = (event: React.KeyboardEvent<HTMLInputElement>) => {
        console.log(event);
        const isModifier = ["Control", "Shift", "Alt", "Meta", "Fn"].includes(
            event.key,
        );
        if (isModifier) {
            return null;
        }

        const parts: string[] = [];
        if (event.ctrlKey) {
            parts.push("Ctrl");
        }
        if (event.altKey) {
            parts.push("Alt");
        }
        if (event.shiftKey) {
            parts.push("Shift");
        }
        if (event.metaKey) {
            parts.push("Meta");
        }

        let key = event.key;
        if (key.length === 1) {
            key = key.toUpperCase();
        } else if (key === " ") {
            key = "Space";
        }

        parts.push(key);
        return parts.join("+");
    };

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

                <h1 className="text-2xl font-semibold">Settings</h1>
            </div>

            <SectionTitle title="Video Source" Icon={TvMinimalPlayIcon}>
                <Select
                    value={form?.video_device_id ?? ""}
                    onValueChange={(value) =>
                        value ? updateForm("video_device_id", value) : null
                    }
                    disabled={
                        !form ||
                        !videoDevices ||
                        connectionStatus !== "connected"
                    }
                >
                    <SelectTrigger className="w-full">
                        <SelectValue placeholder="Select a display" />
                    </SelectTrigger>
                    <SelectContent>
                        {(videoDevices ?? []).map((device) => (
                            <SelectItem key={device.id} value={device.id}>
                                {device.label}
                            </SelectItem>
                        ))}
                    </SelectContent>
                </Select>
            </SectionTitle>

            <SectionTitle title="Audio Source" Icon={Volume2Icon}>
                <div className="grid grid-cols-2 gap-4">
                    <div className="flex justify-between items-center rounded-md border border-border bg-popover px-3 py-2">
                        <p className="text-sm font-medium">System audio</p>
                        <Switch
                            checked={form?.system_audio_enabled ?? false}
                            onCheckedChange={(value: boolean) =>
                                updateForm("system_audio_enabled", value)
                            }
                            disabled={!form || connectionStatus !== "connected"}
                        />
                    </div>

                    <div className="flex flex-col gap-2">
                        <span className="text-sm font-medium">Microphone</span>
                        <Select
                            value={form?.mic_device_id ?? "none"}
                            onValueChange={(value) =>
                                updateForm(
                                    "mic_device_id",
                                    value === "none" ? null : value,
                                )
                            }
                            disabled={
                                !form ||
                                !microphoneDevices ||
                                connectionStatus !== "connected"
                            }
                        >
                            <SelectTrigger className="w-full">
                                <SelectValue placeholder="Select microphone" />
                            </SelectTrigger>
                            <SelectContent>
                                <SelectItem value="none">None</SelectItem>
                                {(microphoneDevices ?? []).map((device) => (
                                    <SelectItem
                                        key={device.id}
                                        value={device.id}
                                    >
                                        {device.label}
                                    </SelectItem>
                                ))}
                            </SelectContent>
                        </Select>
                    </div>
                </div>

                <div className="grid grid-cols-2 gap-4">
                    <div className="flex flex-col gap-2">
                        <div className="flex gap-2 justify-between">
                            <span className="text-sm font-medium">
                                System volume
                            </span>

                            <span className="text-sm font-medium text-muted-foreground">
                                {form?.system_audio_volume ?? 1}
                            </span>
                        </div>

                        <Slider
                            min={0}
                            max={2}
                            step={0.05}
                            value={[form?.system_audio_volume ?? 1]}
                            onValueChange={(value: number | number[]) => {
                                const parsed = Number(
                                    Array.isArray(value) ? value[0] : value,
                                );
                                if (!Number.isNaN(parsed)) {
                                    updateForm("system_audio_volume", parsed);
                                }
                            }}
                            disabled={
                                !form ||
                                !form.system_audio_enabled ||
                                connectionStatus !== "connected"
                            }
                        />
                    </div>

                    <div className="flex flex-col gap-2">
                        <div className="flex gap-2 justify-between">
                            <span className="text-sm font-medium">
                                Mic volume
                            </span>

                            <span className="text-sm font-medium text-muted-foreground">
                                {form?.mic_volume ?? 1}
                            </span>
                        </div>
                        <Slider
                            min={0}
                            max={2}
                            step={0.05}
                            value={[form?.mic_volume ?? 1]}
                            onValueChange={(value: number | number[]) => {
                                const parsed = Number(
                                    Array.isArray(value) ? value[0] : value,
                                );
                                if (!Number.isNaN(parsed)) {
                                    updateForm("mic_volume", parsed);
                                }
                            }}
                            disabled={
                                !form ||
                                !form.mic_device_id ||
                                connectionStatus !== "connected"
                            }
                        />
                    </div>
                </div>
            </SectionTitle>

            <SectionTitle title="Encoder Settings" Icon={BinaryIcon}>
                <div className="grid grid-cols-2 gap-4">
                    <div className="flex flex-col gap-2">
                        <span className="text-sm font-medium">Framerate</span>
                        <Input
                            type="number"
                            min={1}
                            value={
                                typeof form?.framerate === "number"
                                    ? String(form.framerate)
                                    : ""
                            }
                            onChange={(event) => {
                                const parsed = Number(event.target.value);
                                if (!Number.isNaN(parsed)) {
                                    updateForm("framerate", parsed);
                                }
                            }}
                            disabled={!form || connectionStatus !== "connected"}
                        />
                    </div>

                    <div className="flex flex-col gap-2">
                        <span className="text-sm font-medium">
                            Video encoder
                        </span>
                        <Select
                            value={form?.video_encoder_id ?? ""}
                            onValueChange={(value) =>
                                value
                                    ? updateForm("video_encoder_id", value)
                                    : null
                            }
                            disabled={
                                !form ||
                                !encoders ||
                                connectionStatus !== "connected"
                            }
                        >
                            <SelectTrigger className="w-full">
                                <SelectValue placeholder="Select encoder" />
                            </SelectTrigger>
                            <SelectContent>
                                {(encoders ?? []).map((encoder) => {
                                    const suffixParts: string[] = [];

                                    if (encoder.is_hardware) {
                                        suffixParts.push("GPU");
                                    }

                                    if (encoder.required_memory) {
                                        suffixParts.push(
                                            encoder.required_memory,
                                        );
                                    }

                                    const suffix =
                                        suffixParts.length > 0
                                            ? ` (${suffixParts.join(", ")})`
                                            : "";

                                    return (
                                        <SelectItem
                                            key={encoder.id}
                                            value={encoder.id}
                                        >
                                            {encoder.name}
                                            {suffix} ({encoder.id})
                                        </SelectItem>
                                    );
                                })}
                            </SelectContent>
                        </Select>
                    </div>
                </div>

                <div className="flex flex-col gap-2">
                    <span className="text-sm font-medium">Bitrate (kbps)</span>
                    <Slider
                        min={1000}
                        max={20000}
                        step={1000}
                        value={[form?.bitrate_kbps ?? 1000]}
                        onValueChange={(value: number | number[]) => {
                            const parsed = Number(
                                Array.isArray(value) ? value[0] : value,
                            );
                            if (!Number.isNaN(parsed)) {
                                updateForm("bitrate_kbps", parsed);
                            }
                        }}
                        disabled={!form || connectionStatus !== "connected"}
                    />
                </div>
            </SectionTitle>

            <Separator />

            <SectionTitle title="Storage" Icon={DatabaseIcon}>
                <div className="flex gap-3 items-end">
                    <Input
                        value={form?.clips_dir ?? ""}
                        readOnly
                        disabled={!form || connectionStatus !== "connected"}
                    />
                    <Button
                        variant="outline"
                        onClick={async () => {
                            if (!form) {
                                return;
                            }
                            const selected = await open({
                                directory: true,
                                multiple: false,
                                title: "Select clips directory",
                            });
                            if (typeof selected === "string") {
                                updateForm("clips_dir", selected);
                            }
                        }}
                        disabled={!form || connectionStatus !== "connected"}
                    >
                        Choose folder
                    </Button>
                </div>
            </SectionTitle>
            <Separator />

            <SectionTitle title="Shortcuts" Icon={DatabaseIcon}>
                <div className="flex flex-col gap-2">
                    <span className="text-sm font-medium">Clip shortcut</span>
                    <Input
                        value={form?.shortcuts?.clip ?? ""}
                        placeholder="Ctrl+F10"
                        readOnly
                        onClick={() => setIsRecordingShortcut(true)}
                        onFocus={() => setIsRecordingShortcut(true)}
                        onBlur={() => setIsRecordingShortcut(false)}
                        onKeyDown={(event) => {
                            if (!isRecordingShortcut) {
                                return;
                            }
                            if (event.key === "Escape") {
                                setIsRecordingShortcut(false);
                                return;
                            }
                            event.preventDefault();
                            event.stopPropagation();
                            const shortcut = formatShortcut(event);
                            if (!shortcut) {
                                return;
                            }
                            setForm((prev) =>
                                prev
                                    ? {
                                          ...prev,
                                          shortcuts: {
                                              ...prev.shortcuts,
                                              clip: shortcut,
                                          },
                                      }
                                    : prev,
                            );
                            setIsRecordingShortcut(false);
                        }}
                        disabled={!form || connectionStatus !== "connected"}
                    />
                    <span className="text-xs text-muted-foreground">
                        {isRecordingShortcut
                            ? "Press a key combo. Esc to cancel."
                            : "Click to record a shortcut. Ctrl+F10 is the default."}
                    </span>
                </div>
            </SectionTitle>
            <div className="flex justify-end">
                <Button
                    onClick={handleApplySettings}
                    disabled={
                        !form ||
                        settingsMutation.isPending ||
                        connectionStatus !== "connected"
                    }
                >
                    Apply Settings
                </Button>
            </div>
        </section>
    );
};
