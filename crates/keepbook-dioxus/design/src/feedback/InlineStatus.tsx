export interface InlineStatusProps {
  title: string;
  message: string;
}

/** A centered message filling a region that couldn't load, such as a failed fetch. Mirrors `components/feedback/inline_status.rs`. */
export function InlineStatus({ title, message }: InlineStatusProps) {
  return (
    <div className="inline-status">
      <h2>{title}</h2>
      <p>{message}</p>
    </div>
  );
}
