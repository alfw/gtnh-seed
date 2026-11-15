package net.minecraft.world.gen.structure; import java.util.*; import net.minecraft.world.World; import net.minecraft.block.Block;
public abstract class MapGenStructure extends net.minecraft.world.gen.MapGenBase { protected Map field_75053_d = new HashMap();
  protected abstract boolean func_75047_a(int x,int z);
  protected void func_151538_a(World w,int x,int z,int a,int c,Block[] b){ long k=(long)x&4294967295L|((long)z&4294967295L)<<32; if(!field_75053_d.containsKey(k) && func_75047_a(x,z)) field_75053_d.put(k,new StructureStart(x,z)); } }
