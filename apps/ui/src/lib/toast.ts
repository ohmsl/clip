import { toast } from "sonner";

type ToastOptions = {
    title: string;
    description?: string;
    severity?: "success" | "danger" | "warning" | "info";
};

export const addToast = ({ title, description, severity }: ToastOptions) => {
    const message = description ? `${title} — ${description}` : title;
    switch (severity) {
        case "success":
            toast.success(message);
            break;
        case "warning":
            toast.warning(message);
            break;
        case "danger":
            toast.error(message);
            break;
        default:
            toast(message);
    }
};
