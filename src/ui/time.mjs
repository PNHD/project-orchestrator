export function formatTimeLabel(value, locale = undefined) {
  if (!value) return 'Not exposed';

  let timestamp;
  if (value.startsWith('unix:')) {
    const unixSeconds = /^unix:(\d+)$/.exec(value);
    if (!unixSeconds) return 'Not exposed';
    const seconds = Number(unixSeconds[1]);
    if (!Number.isSafeInteger(seconds)) return 'Not exposed';
    timestamp = seconds * 1000;
  } else {
    timestamp = Date.parse(value);
  }
  if (!Number.isFinite(timestamp)) return 'Not exposed';

  const date = new Date(timestamp);
  if (!Number.isFinite(date.getTime())) return 'Not exposed';

  return new Intl.DateTimeFormat(locale, { hour: 'numeric', minute: '2-digit' }).format(date);
}
