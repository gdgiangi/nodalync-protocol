import * as THREE from "three";

/** Frame projected node bounds, leaving room for fixed-size dots and label buttons. */
export function graphCameraFrame(positions, camera, viewport = {}) {
  const points = positions
    .filter((point) => point && [point.x, point.y, point.z].every(Number.isFinite))
    .map((point) => new THREE.Vector3(point.x, point.y, point.z));
  if (!points.length) return null;
  const width = viewport.width || Math.max(camera.aspect, 0.1) * 800;
  const height = viewport.height || 800;
  const direction = new THREE.Vector3(0, 0.96, 0.28).normalize();
  const right = new THREE.Vector3().crossVectors(camera.up, direction).normalize();
  const up = new THREE.Vector3().crossVectors(direction, right).normalize();
  const projected = points.map((point) => new THREE.Vector3(point.dot(right), point.dot(up), point.dot(direction)));
  const bounds = new THREE.Box3().setFromPoints(projected);
  const center = bounds.getCenter(new THREE.Vector3());
  const target = right.clone().multiplyScalar(center.x)
    .addScaledVector(up, center.y).addScaledVector(direction, center.z);
  const verticalTan = Math.tan(THREE.MathUtils.degToRad(camera.fov) / 2);
  const horizontalTan = verticalTan * Math.max(camera.aspect, 0.1);
  // Labels are at most 125px wide and sit 22px below each node.
  const horizontalSpace = Math.max(0.2, 1 - 2 * 72 / width);
  const verticalSpace = Math.max(0.2, 1 - 2 * 48 / height);
  let distance = 18;
  for (const point of projected) {
    const offset = point.clone().sub(center);
    distance = Math.max(distance,
      offset.z + Math.abs(offset.x) / (horizontalTan * horizontalSpace),
      offset.z + Math.abs(offset.y) / (verticalTan * verticalSpace));
  }
  distance *= 1.05;
  const depth = bounds.max.z - bounds.min.z;
  return {
    target,
    position: target.clone().addScaledVector(direction, distance),
    distance,
    far: Math.max(500, distance + depth + 100),
  };
}
