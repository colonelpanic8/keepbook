export interface InlineStatusProps {
  title: string;
  message: string;
}

/** A centered message filling a region that couldn't load, such as a failed fetch. Mirrors `InlineStatus` in `views/shared.rs`. */
export function InlineStatus({ title, message }: InlineStatusProps) {
  return (
    <div className="inline-status">
      <h2>{title}</h2>
      <p>{message}</p>
    </div>
  );
}
