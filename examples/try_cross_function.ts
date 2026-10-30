function fail() {
  throw 7;
}
try {
  fail();
} catch (error) {
  console.log(error);
}
console.log("after");
