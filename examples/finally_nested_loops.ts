let log = [];
let index = 0;
while (index < 2) {
  try {
    try {
      break;
    } finally {
      log.push("inner");
    }
  } finally {
    log.push("outer");
  }
  index = index + 1;
}
console.log(log.join(","));
let out = "";
for (let step = 0; step < 2; step = step + 1) {
  try {
    try {
      continue;
    } finally {
      out = out + "i";
    }
  } finally {
    out = out + "o";
  }
  out = out + "x";
}
console.log(out);
