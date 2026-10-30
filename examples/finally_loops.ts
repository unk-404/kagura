let log = [];
let index = 0;
while (index < 3) {
  try {
    if (index === 1) {
      break;
    }
    log.push(index);
  } finally {
    log.push("f" + index);
  }
  index = index + 1;
}
console.log(log.join(","));
let out = "";
for (let step = 0; step < 3; step = step + 1) {
  try {
    if (step === 1) {
      continue;
    }
    out = out + step;
  } finally {
    out = out + "f";
  }
}
console.log(out);
