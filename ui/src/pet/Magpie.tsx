// The magpie, drawn as SVG so it stays crisp at any DPI and animates with CSS.
// Black head and back, white belly and wing patch, blue-sheened wing and tail.

export type Mood = "idle" | "alert" | "sleep";

export function Magpie({ mood }: { mood: Mood }) {
  const eyesClosed = mood === "sleep";
  return (
    <svg className={`magpie mood-${mood}`} viewBox="0 0 160 160" aria-label="Magpie">
      <g className="tail">
        <path d="M58 104 L10 138 L18 146 L66 114 Z" fill="#1d3550" />
        <path d="M58 104 L14 140 L20 144" stroke="#3e8cdc" strokeWidth="2" fill="none" opacity=".7" />
      </g>
      <g className="body">
        <ellipse cx="86" cy="96" rx="40" ry="30" fill="#141a22" />
        <ellipse cx="92" cy="104" rx="28" ry="20" fill="#f5f7fa" />
        <path className="wing" d="M58 88 Q84 70 108 92 Q90 112 60 104 Z" fill="#23466e" />
        <path d="M70 92 Q86 86 98 96 Q86 100 72 98 Z" fill="#f5f7fa" opacity=".9" />
        <line x1="84" y1="124" x2="80" y2="140" stroke="#2b2f36" strokeWidth="3" strokeLinecap="round" />
        <line x1="98" y1="124" x2="100" y2="140" stroke="#2b2f36" strokeWidth="3" strokeLinecap="round" />
      </g>
      <g className="head">
        <circle cx="116" cy="66" r="22" fill="#141a22" />
        <path d="M136 62 L154 68 L136 72 Z" fill="#3a3f47" />
        {eyesClosed ? (
          <path d="M118 62 Q123 66 128 62" stroke="#f5f7fa" strokeWidth="2.5" fill="none" strokeLinecap="round" />
        ) : (
          <>
            <circle cx="123" cy="61" r="5" fill="#f5f7fa" />
            <circle className="pupil" cx="124.5" cy="61" r="2.6" fill="#0b0f14" />
          </>
        )}
      </g>
      {mood === "sleep" && (
        <text className="zzz" x="130" y="36" fill="#8a99aa" fontSize="16" fontWeight="700">
          z
        </text>
      )}
    </svg>
  );
}
