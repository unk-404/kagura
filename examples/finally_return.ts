function withFinally() {
  try {
    return 1;
  } finally {
    console.log("a");
  }
}
console.log(withFinally());
function overrides() {
  try {
    return 2;
  } finally {
    return 3;
  }
}
console.log(overrides());
function fromCatch() {
  try {
    throw "x";
  } catch (error) {
    return 4;
  } finally {
    console.log("c");
  }
}
console.log(fromCatch());
try {
  try {
    throw "boom";
  } finally {
    console.log("inner");
  }
} catch (error) {
  console.log(error);
}
try {
  try {
    console.log("body");
  } finally {
    console.log("finally");
  }
} finally {
  console.log("outer");
}
try {
  try {
    throw "start";
  } catch (error) {
    throw "replaced";
  } finally {
    console.log("catch finally");
  }
} catch (error) {
  console.log(error);
}
