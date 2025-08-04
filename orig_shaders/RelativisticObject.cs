using System;
using UnityEngine;

public class RelativisticObject : MonoBehaviour
{
	private MeshFilter meshFilter;

	private Vector3[] rawVerts;

	public Vector3 viw;

	private GameState state;

	private float startTime;

	private float deathTime;

	private Vector3 oneVert;

	public void SetStartTime()
	{
		startTime = (float)GameObject.FindGameObjectWithTag("Player").GetComponent<GameState>().TotalTimeWorld;
	}

	public void SetDeathTime()
	{
		deathTime = (float)state.TotalTimeWorld;
	}

	private void Start()
	{
		checkSpeed();
		state = GameObject.FindGameObjectWithTag("Player").GetComponent<GameState>();
		meshFilter = GetComponent<MeshFilter>();
		MeshRenderer component = GetComponent<MeshRenderer>();
		if (component != null && component.materials[0].mainTexture != null)
		{
			Material material = UnityEngine.Object.Instantiate(component.materials[0]);
			component.materials[0] = material;
			component.materials[0].SetFloat("_strtTime", startTime);
			component.materials[0].SetVector("_strtPos", new Vector4(base.transform.position.x, base.transform.position.y, base.transform.position.z, 0f));
		}
		if (meshFilter != null)
		{
			rawVerts = meshFilter.mesh.vertices;
		}
		else
		{
			rawVerts = null;
		}
		Transform transform = Camera.main.transform;
		float num = (Camera.main.farClipPlane - Camera.main.nearClipPlane) / 2f;
		Vector3 center = transform.position + transform.forward * num;
		float num2 = 500000f;
		meshFilter.sharedMesh.bounds = new Bounds(center, Vector3.one * num2);
	}

	private void Update()
	{
		MeshRenderer component = GetComponent<MeshRenderer>();
		if (meshFilter != null && !state.MovementFrozen)
		{
			ObjectMeshDensity component2 = GetComponent<ObjectMeshDensity>();
			if (component2 != null && rawVerts != null && component2.change != null)
			{
				if (!component2.state && RecursiveTransform(rawVerts[0], meshFilter.transform).magnitude < 21000f)
				{
					if (component2.ReturnVerts(meshFilter.mesh, Subdivide: true))
					{
						rawVerts = new Vector3[meshFilter.mesh.vertices.Length];
						Array.Copy(meshFilter.mesh.vertices, rawVerts, meshFilter.mesh.vertices.Length);
					}
				}
				else if (component2.state && RecursiveTransform(rawVerts[0], meshFilter.transform).magnitude > 21000f && component2.ReturnVerts(meshFilter.mesh, Subdivide: false))
				{
					rawVerts = new Vector3[meshFilter.mesh.vertices.Length];
					Array.Copy(meshFilter.mesh.vertices, rawVerts, meshFilter.mesh.vertices.Length);
				}
			}
			if (component != null)
			{
				Vector3 vector = viw / (float)state.SpeedOfLight;
				component.materials[0].SetVector("_viw", new Vector4(vector.x, vector.y, vector.z, 0f));
			}
			if (base.transform != null && deathTime != 0f)
			{
				float num = 57.29578f * Mathf.Acos(Vector3.Dot(state.PlayerVelocityVector, Vector3.forward) / state.PlayerVelocityVector.magnitude);
				if (state.PlayerVelocityVector.sqrMagnitude == 0f)
				{
					num = 0f;
				}
				Quaternion quaternion = Quaternion.AngleAxis(0f - num, Vector3.Cross(state.PlayerVelocityVector, Vector3.forward));
				Vector3 vector2 = new Vector3(base.transform.position.x, base.transform.position.y, base.transform.position.z);
				vector2 -= state.playerTransform.position;
				vector2 = quaternion * vector2;
				Vector3 vector3 = quaternion * viw;
				float num2 = 0f - Vector3.Dot(vector2, vector2);
				float num3 = 0f - 2f * Vector3.Dot(vector2, vector3);
				float num4 = (float)state.SpeedOfLightSqrd - Vector3.Dot(vector3, vector3);
				float num5 = (float)(((double)(0f - num3) - Math.Sqrt(num3 * num3 - 4f * num4 * num2)) / (double)(2f * num4));
				if (state.TotalTimeWorld + (double)num5 > (double)deathTime)
				{
					UnityEngine.Object.Destroy(base.gameObject);
				}
			}
			_ = viw;
			if (GetComponent<Rigidbody>() != null && !double.IsNaN(state.SqrtOneMinusVSquaredCWDividedByCSquared) && (float)state.SqrtOneMinusVSquaredCWDividedByCSquared != 0f)
			{
				Vector3 velocity = viw;
				velocity.x /= (float)state.SqrtOneMinusVSquaredCWDividedByCSquared;
				velocity.y /= (float)state.SqrtOneMinusVSquaredCWDividedByCSquared;
				velocity.z /= (float)state.SqrtOneMinusVSquaredCWDividedByCSquared;
				GetComponent<Rigidbody>().velocity = velocity;
			}
		}
		else if (meshFilter != null && component != null && GetComponent<Rigidbody>() != null)
		{
			GetComponent<Rigidbody>().velocity = Vector3.zero;
		}
	}

	public Vector3 RecursiveTransform(Vector3 pt, Transform trans)
	{
		Vector3 zero = Vector3.zero;
		if (trans.parent != null)
		{
			pt = RecursiveTransform(zero, trans.parent);
			return pt;
		}
		return trans.TransformPoint(pt);
	}

	private void checkSpeed()
	{
		if (viw.magnitude > 4.95f)
		{
			viw = viw.normalized * 4.95f;
		}
	}
}
