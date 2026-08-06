interface TileProps {
  label: string;
  value: string | number;
  sub?: string;
}

/** One statistic. Value first: the grid is meant to be read by scanning the big numbers. */
export function Tile({ label, value, sub }: TileProps) {
  return (
    <div className="tile">
      <span className="tile-v">{value}</span>
      <span className="tile-l">{label}</span>
      {sub ? <span className="tile-s">{sub}</span> : null}
    </div>
  );
}
