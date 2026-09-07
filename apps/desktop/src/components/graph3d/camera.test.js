import test from "node:test";
import assert from "node:assert/strict";
import * as THREE from "three";
import { graphCameraFrame } from "./camera.js";

test("fit graph keeps wide, offset node bounds inside portrait and landscape views", () => {
  const points = [{ x: -80, y: -8, z: -10 }, { x: 95, y: 8, z: 25 }, { x: 60, y: 0, z: -45 }];
  for (const aspect of [0.5, 1, 2]) {
    const camera = new THREE.PerspectiveCamera(50, aspect, 0.1, 500);
    const frame = graphCameraFrame(points, camera);
    camera.position.copy(frame.position);
    camera.far = frame.far;
    camera.lookAt(frame.target);
    camera.updateProjectionMatrix();
    camera.updateMatrixWorld();
    for (const point of points) {
      const projected = new THREE.Vector3(point.x, point.y, point.z).project(camera);
      assert.ok(Math.abs(projected.x) < 0.9 && Math.abs(projected.y) < 0.9);
      assert.ok(projected.z > -1 && projected.z < 1);
    }
  }
});

test("fit graph handles a single entity and ignores invalid coordinates", () => {
  const camera = new THREE.PerspectiveCamera(50, 1, 0.1, 500);
  assert.equal(graphCameraFrame([], camera), null);
  assert.equal(graphCameraFrame([{ x: NaN, y: 0, z: 0 }], camera), null);
  const frame = graphCameraFrame([{ x: 5, y: 0, z: 20 }], camera);
  assert.ok(frame.distance >= 18);
  assert.deepEqual(frame.target.toArray(), [5, 0, 20]);
});


test("a planar graph uses the viewport instead of fitting an empty bounding sphere", () => {
  const width = 890, height = 450;
  const camera = new THREE.PerspectiveCamera(50, width / height, 0.1, 500);
  const points = Array.from({ length: 13 }, (_, index) => ({
    x: Math.cos(index / 13 * Math.PI * 2) * 32,
    y: 0,
    z: Math.sin(index / 13 * Math.PI * 2) * 27,
  }));
  const frame = graphCameraFrame(points, camera, { width, height });
  camera.position.copy(frame.position);
  camera.lookAt(frame.target);
  camera.updateProjectionMatrix();
  camera.updateMatrixWorld();
  const screen = points.map((point) => new THREE.Vector3(point.x, point.y, point.z).project(camera));
  const bounds = new THREE.Box3().setFromPoints(screen);
  assert.ok(bounds.max.y - bounds.min.y > 1.35, "graph should use at least two-thirds of viewport height");
  for (const point of screen) {
    assert.ok(Math.abs(point.x) < 1 - 2 * 65 / width, "leave label width margin");
    assert.ok(Math.abs(point.y) < 1 - 2 * 40 / height, "leave label height margin");
  }
});
