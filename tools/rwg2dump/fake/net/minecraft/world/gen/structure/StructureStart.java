package net.minecraft.world.gen.structure; import java.util.LinkedList;
public class StructureStart { protected LinkedList components = new LinkedList(); protected StructureBoundingBox field_75074_b; private int cx, cz;
  public StructureStart(int cx,int cz){this.cx=cx;this.cz=cz; components.add(new StructureComponent(cx*16,cz*16)); components.add(new StructureComponent(cx*16+8,cz*16)); field_75074_b=new StructureBoundingBox(cx*16,60,cz*16,cx*16+20,70,cz*16+20);}
  public boolean func_75069_d(){ return components.size()>2; } public int func_143019_e(){return cx;} }
