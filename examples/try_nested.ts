try {
  try {
    throw 1;
  } catch (inner) {
    throw inner + 1;
  }
} catch (outer) {
  console.log(outer);
}
