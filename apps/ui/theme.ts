/**
 * A numeric shade key used by HeroUI semantic palettes.
 */
type Shade =
    | "50"
    | "100"
    | "200"
    | "300"
    | "400"
    | "500"
    | "600"
    | "700"
    | "800"
    | "900";

/**
 * A HeroUI semantic colour palette (50-900).
 */
type SemanticPalette = Record<Shade | "foreground" | "DEFAULT", string>;

/**
 * The semantic colours object HeroUI expects inside a theme entry.
 * Kept intentionally tight to avoid accidental `any`.
 */
type HeroUiThemeColors = Partial<{
    background: string;
    foreground: string;
    divider: string;
    focus: string;
    content1: string;
    content2: string;
    content3: string;
    content4: string;
    default: SemanticPalette;
    primary: SemanticPalette;
    secondary: SemanticPalette;
}>;

/**
 * Your brand palettes derived from:
 * White Smoke:     #F7F7F7
 * Eigengrau:       #0C1421
 * Medium Sapphire: #3863A8
 * Aero:            #7CB9F2
 */
const defaultPalette: SemanticPalette = {
    "50": "#09090b",
    "100": "#18181b",
    "200": "#27272a",
    "300": "#3f3f46",
    "400": "#52525b",
    "500": "#71717a",
    "600": "#a1a1aa",
    "700": "#d4d4d8",
    "800": "#e4e4e7",
    "900": "#f4f4f5",
    foreground: "#FFFFFF",
    DEFAULT: "#71717a",
};

const primaryPalette: SemanticPalette = {
    "50": "#fee2e2",
    "100": "#fecaca",
    "200": "#fca5a5",
    "300": "#f87171",
    "400": "#ef4444",
    "500": "#e7000b",
    "600": "#b91c1c",
    "700": "#991b1b",
    "800": "#7f1d1d",
    "900": "#450a0a",
    foreground: "#FFFFFF",
    DEFAULT: "#e7000b",
};

const secondaryPalette: SemanticPalette = {
    "50": "#F1F8FE",
    "100": "#E5F1FD",
    "200": "#CBE4FB",
    "300": "#B2D7F9",
    "400": "#8FC5F6",
    "500": "#7CB9F2",
    "600": "#6AA0CE",
    "700": "#5887AB",
    "800": "#476E87",
    "900": "#355564",
    foreground: "#FFFFFF",
    DEFAULT: "#7CB9F2",
};

export const lightColors: HeroUiThemeColors = {
    // Layout tokens
    background: "#F7F7F7", // White Smoke
    foreground: "#0C1421", // Eigengrau
    divider: defaultPalette["200"],
    focus: secondaryPalette["500"],

    // Content surfaces
    content1: "#FFFFFF",
    content2: defaultPalette["50"],
    content3: defaultPalette["100"],
    content4: defaultPalette["200"],

    // Base semantic palettes
    default: defaultPalette,
    primary: primaryPalette,
    secondary: secondaryPalette,
};

export const darkColors: HeroUiThemeColors = {
    primary: primaryPalette,
    secondary: secondaryPalette,

    // Layout tokens
    background: "#09090b", // Eigengrau
    foreground: "#F7F7F7", // White Smoke
    focus: primaryPalette["100"],
};
