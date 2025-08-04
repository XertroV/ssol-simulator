using UnityEngine;

public class RelativisticParent : MonoBehaviour
{
	private MeshFilter meshFilter;

	public Vector3 viw;

	private GameState state;

	public int amp;

	public float period;

	public bool periodic;

	private void Start()
	{
		if ((bool)GetComponent<ObjectMeshDensity>())
		{
			GetComponent<ObjectMeshDensity>().enabled = false;
		}
		int num = 0;
		int num2 = 0;
		checkSpeed();
		Matrix4x4 worldToLocalMatrix = base.transform.worldToLocalMatrix;
		MeshFilter[] componentsInChildren = GetComponentsInChildren<MeshFilter>();
		int[] array = new int[componentsInChildren.Length];
		MeshRenderer[] componentsInChildren2 = GetComponentsInChildren<MeshRenderer>();
		int num3 = componentsInChildren.Length;
		int num4 = 0;
		for (int i = 0; i < num3; i++)
		{
			if (!(componentsInChildren[i] == null) && !(componentsInChildren[i].sharedMesh == null))
			{
				num += componentsInChildren[i].sharedMesh.vertices.Length;
				num2 += componentsInChildren[i].sharedMesh.triangles.Length;
				array[i] = componentsInChildren[i].mesh.subMeshCount;
				num4 += componentsInChildren[i].mesh.subMeshCount;
			}
		}
		Vector3[] array2 = new Vector3[num];
		int[][] array3 = new int[num4][];
		for (int j = 0; j < num4; j++)
		{
			array3[j] = new int[num2];
		}
		Vector2[] array4 = new Vector2[num];
		Material[] array5 = new Material[num4];
		int num5 = 0;
		int num6 = 0;
		for (int k = 0; k < num3; k++)
		{
			Mesh sharedMesh = componentsInChildren[k].sharedMesh;
			if (sharedMesh == null)
			{
				continue;
			}
			for (int l = 0; l < array[k]; l++)
			{
				array5[num6] = componentsInChildren2[k].materials[l];
				int[] triangles = sharedMesh.GetTriangles(l);
				for (int m = 0; m < triangles.Length; m++)
				{
					array3[num6][m] = triangles[m] + num5;
				}
				num6++;
			}
			Matrix4x4 matrix4x = worldToLocalMatrix * componentsInChildren[k].transform.localToWorldMatrix;
			for (int n = 0; n < sharedMesh.vertices.Length; n++)
			{
				array2[num5] = matrix4x.MultiplyPoint3x4(sharedMesh.vertices[n]);
				array4[num5] = sharedMesh.uv[n];
				num5++;
			}
			componentsInChildren[k].gameObject.SetActive(value: false);
		}
		Mesh mesh = new Mesh();
		mesh.subMeshCount = num4;
		mesh.vertices = array2;
		num6 = 0;
		for (int num7 = 0; num7 < num3; num7++)
		{
			for (int num8 = 0; num8 < array[num7]; num8++)
			{
				mesh.SetTriangles(array3[num6], num6);
				num6++;
			}
		}
		mesh.uv = array4;
		GetComponent<MeshFilter>().mesh = mesh;
		GetComponent<MeshRenderer>().enabled = true;
		GetComponent<MeshFilter>().mesh.RecalculateNormals();
		GetComponent<MeshFilter>().GetComponent<Renderer>().materials = array5;
		base.transform.gameObject.SetActive(value: true);
		meshFilter = GetComponent<MeshFilter>();
		state = GameObject.FindGameObjectWithTag("Player").GetComponent<GameState>();
		meshFilter = GetComponent<MeshFilter>();
		MeshRenderer component = GetComponent<MeshRenderer>();
		if (component.materials[0].mainTexture != null)
		{
			Material material = Object.Instantiate(component.materials[0]);
			material.SetFloat("_viw", 0f);
			component.materials[0] = material;
		}
		Transform transform = Camera.main.transform;
		float num9 = (Camera.main.farClipPlane - Camera.main.nearClipPlane) / 2f;
		Vector3 center = transform.position + transform.forward * num9;
		float num10 = 500000f;
		meshFilter.sharedMesh.bounds = new Bounds(center, Vector3.one * num10);
		if ((bool)GetComponent<ObjectMeshDensity>())
		{
			GetComponent<ObjectMeshDensity>().enabled = true;
		}
	}

	private void Update()
	{
		if (meshFilter != null)
		{
			_ = state.MovementFrozen;
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

	public void PeriodicAddTime()
	{
		meshFilter.transform.Translate(new Vector3(0f, (float)amp * Mathf.Sin((float)((double)period * state.TotalTimeWorld)) - (float)amp * Mathf.Sin((float)((double)period * (state.TotalTimeWorld - state.DeltaTimeWorld))), 0f));
	}

	public Vector3 PeriodicSubtractTime(float tisw, Quaternion rotation)
	{
		return rotation * new Vector3(0f, (float)amp * Mathf.Sin((float)((double)period * (state.TotalTimeWorld + (double)tisw))) - (float)amp * Mathf.Sin((float)((double)period * state.TotalTimeWorld)), 0f);
	}

	public Vector3 CurrentVelocity()
	{
		Vector3 zero = Vector3.zero;
		zero.y = (float)amp * period * Mathf.Cos((float)((double)period * state.TotalTimeWorld));
		return zero;
	}

	public Vector4 CurrentVelocity4()
	{
		Vector4 zero = Vector4.zero;
		zero.y = (float)amp * period * Mathf.Cos((float)((double)period * state.TotalTimeWorld));
		return zero;
	}

	private void checkSpeed()
	{
		if (periodic && (float)amp * period > 4.95f)
		{
			period = 4.95f / (float)amp;
		}
		else if (viw.magnitude > 4.95f)
		{
			viw = viw.normalized * 4.95f;
		}
	}
}
