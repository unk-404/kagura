var date = new Date(0);
console.log(date.getTime());
console.log(date.toISOString());
console.log(date.getUTCFullYear(), date.getUTCMonth(), date.getUTCDate(), date.getUTCDay());
var parsed = new Date("2020-01-02T03:04:05.006Z");
console.log(parsed.toISOString(), parsed.getUTCHours(), parsed.getUTCMinutes(), parsed.getUTCSeconds(), parsed.getUTCMilliseconds());
console.log(Date.parse("2020-01-02T03:04:05.006Z"));
console.log(Date.UTC(2020, 0, 2, 3, 4, 5, 6));
