/**
 * One project-wide word list, used twice: as whisper's initial prompt so it
 * spells the names right, and as the seed of the chaptr roster.
 */
export function Vocabulary({
  value,
  onChange,
}: {
  value: string;
  onChange: (v: string) => void;
}) {
  return (
    <label className="field">
      <b>Names and words it should know</b>
      <span className="note">
        Speech-to-text mangles names and jargon it has never seen. Listing them
        here gives it the right spellings up front. The same list goes to the
        chaptr model, so it can name people instead of saying "a player".
      </span>
      <input
        className="wide"
        placeholder="names of people, places, characters, in-jokes"
        value={value}
        onChange={(e) => onChange(e.target.value)}
      />
    </label>
  );
}
