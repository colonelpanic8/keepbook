import paths from "../../../assets/icons.json";

/** Stroke icons on a 24px grid (Lucide shapes), from the shared `assets/icons.json`. */
export type IconName = keyof typeof paths;

export function renderIcon(icon: IconName) {
  return (
    <svg
      className="button-icon"
      viewBox="0 0 24 24"
      width="14"
      height="14"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      {paths[icon].map((d) => (
        <path key={d} d={d} />
      ))}
    </svg>
  );
}
