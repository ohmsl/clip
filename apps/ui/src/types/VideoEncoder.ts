export type VideoEncoder = {
    id: string;
    name: string;
    description: string;
    is_hardware: boolean;
    required_memory?: string | null;
};
